import Foundation

#if os(macOS)
struct ShellPaneProjectionService {
    private let fileManager: FileManager
    private let iso8601Formatter = ISO8601DateFormatter()

    init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
    }

    func needsBootContextProjection(_ pane: ShellPane) -> Bool {
        guard let context = pane.context else { return true }
        return context.controlPath == nil
            || context.alanBindingFile == nil
            || context.launchStrategy == nil
    }

    func projectedAttention(
        metadataAttention: ShellAttentionState,
        processExited: Bool,
        binding: ShellAlanBinding?,
        surfaceState: AlanTerminalSurfaceStateSnapshot? = nil
    ) -> ShellAttentionState {
        if binding?.pendingRequest == true || processExited || surfaceState?.childExited == true {
            return .awaitingUser
        }

        if surfaceState?.rendererHealth == "failed"
            || surfaceState?.readiness == .unready(reason: .rendererFailed)
        {
            return .notable
        }

        return metadataAttention
    }

    func projectedViewport(
        current: ShellPane,
        metadata: TerminalPaneMetadataSnapshot,
        runtime: TerminalHostRuntimeSnapshot
    ) -> ShellViewportSnapshot {
        ShellViewportSnapshot(
            title: metadata.title ?? current.viewport?.title,
            summary: projectedViewportSummary(
                metadata: metadata,
                runtime: runtime,
                fallback: current.viewport?.summary
            ),
            visibleExcerpt: current.viewport?.visibleExcerpt,
            lastActivityAt: metadata.lastUpdatedAt.map(iso8601Formatter.string)
                ?? current.viewport?.lastActivityAt
                ?? iso8601Formatter.string(from: runtime.lastUpdatedAt)
        )
    }

    func projectedProcessExited(
        metadataProcessExited: Bool?,
        surfaceState: AlanTerminalSurfaceStateSnapshot?
    ) -> Bool? {
        if metadataProcessExited == true || surfaceState?.childExited == true {
            return true
        }

        return metadataProcessExited ?? surfaceState?.childExited
    }

    func projectedContext(
        for pane: ShellPane,
        bootProfile: AlanShellBootProfile,
        workingDirectory: String?,
        processExited: Bool?,
        lastCommandExitCode: Int?,
        lastMetadataAt: Date?,
        activeTaskState: ShellTabActiveTaskState? = nil,
        existing: ShellContextSnapshot?,
        runtime: TerminalHostRuntimeSnapshot? = nil
    ) -> ShellContextSnapshot {
        let resolvedWorkingDirectory = workingDirectory ?? pane.cwd ?? bootProfile.workingDirectory
        let repositoryContext = repositoryContext(for: resolvedWorkingDirectory)
        let projectedProcessExited = projectedProcessExited(
            metadataProcessExited: processExited,
            surfaceState: runtime?.surfaceState
        )
        let bootProfileHasTerminalProfileState = bootProfile.environment.keys.contains(
            "ALAN_TERMINAL_PROFILE_STATE"
        )
        let terminalProfileState = bootProfile.environment["ALAN_TERMINAL_PROFILE_STATE"]
            ?? existing?.terminalProfileState
        let terminalProfileRequestedID = bootProfileHasTerminalProfileState
            ? bootProfile.environment["ALAN_TERMINAL_PROFILE_REQUESTED_ID"]
            : existing?.terminalProfileRequestedID
        let terminalProfileID = bootProfileHasTerminalProfileState
            ? bootProfile.environment["ALAN_TERMINAL_PROFILE_ID"]
            : existing?.terminalProfileID
        let terminalProfileKind = bootProfileHasTerminalProfileState
            ? bootProfile.environment["ALAN_TERMINAL_PROFILE_KIND"]
            : existing?.terminalProfileKind
        let terminalProfileTitle = bootProfileHasTerminalProfileState
            ? bootProfile.environment["ALAN_TERMINAL_PROFILE_TITLE"]
            : existing?.terminalProfileTitle

        return ShellContextSnapshot(
            workingDirectoryName: workingDirectoryName(for: resolvedWorkingDirectory)
                ?? existing?.workingDirectoryName,
            repositoryRoot: repositoryContext.repositoryRoot ?? existing?.repositoryRoot,
            gitBranch: repositoryContext.gitBranch ?? existing?.gitBranch,
            controlPath: bootProfile.environment["ALAN_SHELL_CONTROL_DIR"] ?? existing?.controlPath,
            socketPath: bootProfile.environment["ALAN_SHELL_SOCKET"] ?? existing?.socketPath,
            alanBindingFile: bootProfile.environment["ALAN_SHELL_BINDING_FILE"]
                ?? existing?.alanBindingFile,
            launchCommand: bootProfile.launchCommandString,
            launchStrategy: bootProfile.command.strategy.rawValue,
            terminalProfileState: terminalProfileState,
            terminalProfileRequestedID: terminalProfileRequestedID,
            terminalProfileID: terminalProfileID,
            terminalProfileKind: terminalProfileKind,
            terminalProfileTitle: terminalProfileTitle,
            shellIntegrationSource: "ghostty_shell_integration",
            processState: projectedProcessState(
                processExited: projectedProcessExited,
                activeTaskState: activeTaskState,
                existing: existing?.processState
            ),
            rendererPhase: runtime?.renderer.phase.rawValue ?? existing?.rendererPhase,
            rendererHealth: runtime?.surfaceState.rendererHealth
                ?? runtime?.renderer.phase.rawValue
                ?? existing?.rendererHealth,
            surfaceReadiness: runtime.map { surfaceReadinessValue($0.surfaceState.readiness) }
                ?? existing?.surfaceReadiness,
            inputReady: runtime?.surfaceState.inputReady ?? existing?.inputReady,
            readonly: runtime?.surfaceState.readonly ?? existing?.readonly,
            terminalMode: runtime?.surfaceState.terminalMode.rawValue ?? existing?.terminalMode,
            displayName: runtime?.displayName ?? existing?.displayName,
            displayID: runtime?.displayID ?? existing?.displayID,
            windowTitle: runtime?.attachedWindowTitle ?? existing?.windowTitle,
            lastMetadataAt: lastMetadataAt.map(iso8601Formatter.string)
                ?? existing?.lastMetadataAt,
            lastCommandExitCode: lastCommandExitCode ?? existing?.lastCommandExitCode
        )
    }

    func projectedAlanBinding(
        for pane: ShellPane,
        binding: ShellAlanBinding?,
        processExited: Bool
    ) -> ShellAlanBinding? {
        if let binding {
            return binding
        }

        if let existing = pane.alanBinding {
            return existing
        }

        return nil
    }

    private func projectedViewportSummary(
        metadata: TerminalPaneMetadataSnapshot,
        runtime: TerminalHostRuntimeSnapshot,
        fallback: String?
    ) -> String? {
        if metadata.processExited || runtime.surfaceState.childExited {
            if let exitCode = metadata.lastCommandExitCode {
                return "Exited \(exitCode)"
            }
            return "Exited"
        }

        if runtime.surfaceState.rendererHealth == "failed"
            || runtime.renderer.phase == .failed
            || runtime.surfaceState.readiness == .unready(reason: .rendererFailed)
        {
            return "Renderer failed"
        }

        if runtime.surfaceState.readonly {
            return "Read-only"
        }

        if runtime.surfaceState.inputReady == false,
           runtime.surfaceState.readiness == .unready(reason: .inputNotReady)
        {
            return "Starting"
        }

        return metadata.summary ?? fallback
    }

    private func surfaceReadinessValue(_ readiness: AlanTerminalSurfaceReadiness) -> String {
        switch readiness {
        case .ready:
            return "ready"
        case .unready(let reason):
            return reason.rawValue
        }
    }

    private func projectedProcessState(
        processExited: Bool?,
        activeTaskState: ShellTabActiveTaskState?,
        existing: String?
    ) -> String? {
        if processExited == true {
            return "exited"
        }

        if activeTaskState == .foregroundCommand {
            return "foreground_command"
        }

        if processExited == false {
            return "running"
        }

        return existing
    }

    private func workingDirectoryName(for path: String?) -> String? {
        guard let path, !path.isEmpty else { return nil }
        let lastComponent = URL(fileURLWithPath: path).lastPathComponent
        return lastComponent.isEmpty ? path : lastComponent
    }

    private func repositoryContext(for workingDirectory: String?) -> (
        repositoryRoot: String?,
        gitBranch: String?
    ) {
        guard let workingDirectory, !workingDirectory.isEmpty else {
            return (nil, nil)
        }

        var currentURL = URL(fileURLWithPath: workingDirectory, isDirectory: true).standardizedFileURL
        var isDirectory: ObjCBool = false

        if !fileManager.fileExists(atPath: currentURL.path, isDirectory: &isDirectory) {
            return (nil, nil)
        }

        if !isDirectory.boolValue {
            currentURL.deleteLastPathComponent()
        }

        while true {
            let gitEntryURL = currentURL.appendingPathComponent(".git")
            if fileManager.fileExists(atPath: gitEntryURL.path) {
                let gitDirectoryURL = resolveGitDirectory(for: gitEntryURL, repositoryRoot: currentURL)
                let gitBranch = gitDirectoryURL.flatMap(readGitBranch(from:))
                return (currentURL.path, gitBranch)
            }

            let parentURL = currentURL.deletingLastPathComponent()
            if parentURL.path == currentURL.path {
                return (nil, nil)
            }
            currentURL = parentURL
        }
    }

    private func resolveGitDirectory(for gitEntryURL: URL, repositoryRoot: URL) -> URL? {
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: gitEntryURL.path, isDirectory: &isDirectory) else {
            return nil
        }

        if isDirectory.boolValue {
            return gitEntryURL
        }

        guard let content = try? String(contentsOf: gitEntryURL, encoding: .utf8) else {
            return nil
        }

        let prefix = "gitdir:"
        guard content.hasPrefix(prefix) else { return nil }
        let rawPath = content.dropFirst(prefix.count).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !rawPath.isEmpty else { return nil }

        let pathURL = URL(fileURLWithPath: rawPath, relativeTo: repositoryRoot)
        return pathURL.standardizedFileURL
    }

    private func readGitBranch(from gitDirectoryURL: URL) -> String? {
        let headURL = gitDirectoryURL.appendingPathComponent("HEAD")
        guard let head = try? String(contentsOf: headURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !head.isEmpty
        else {
            return nil
        }

        let refPrefix = "ref: "
        if head.hasPrefix(refPrefix) {
            let reference = String(head.dropFirst(refPrefix.count))
            return reference.split(separator: "/").last.map(String.init)
        }

        return "detached:\(String(head.prefix(12)))"
    }
}
#endif
