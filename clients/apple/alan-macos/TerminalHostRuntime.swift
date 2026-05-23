import CoreGraphics
import Foundation

#if os(macOS)
private func inferredAlanRepoRoot(from filePath: String = #filePath) -> String? {
    var url = URL(fileURLWithPath: filePath)
    for _ in 0..<4 {
        url.deleteLastPathComponent()
    }
    return url.path
}

struct GhosttyDependencyCandidate: Identifiable, Equatable {
    let label: String
    let path: String
    let isPresent: Bool

    var id: String { path }
}

struct AlanCommandCandidate: Identifiable, Equatable {
    let label: String
    let path: String
    let isPresent: Bool

    var id: String { "\(label):\(path)" }
}

enum AlanLaunchStrategy: String, Equatable {
    case shellCommandEnv = "shell_command_env"
    case loginShellOverride = "login_shell_override"
    case loginShellEnv = "login_shell_env"
    case loginShellFallback = "login_shell_fallback"
    case envOverride = "env_override"
    case repoDebugBinary = "repo_debug_binary"
    case repoReleaseBinary = "repo_release_binary"
    case bundledResourceBinary = "bundled_resource_binary"
    case pathBinary = "path_binary"
    case shellLookup = "shell_lookup"
}

struct AlanCommandResolution: Equatable {
    let strategy: AlanLaunchStrategy
    let executablePath: String?
    let launchPath: String
    let arguments: [String]
    let bootCommand: String
    let surfaceCommand: String?
    let summary: String
    let detail: String?
    let repoRoot: String?
    let candidates: [AlanCommandCandidate]

    var launchCommandString: String {
        ([launchPath] + arguments).map(AlanShellBootProfile.shellQuoted).joined(separator: " ")
    }

    static func resolve(
        for launchTarget: ShellLaunchTarget,
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> AlanCommandResolution {
        switch launchTarget {
        case .shell:
            return resolveShell(fileManager: fileManager, environment: environment)
        case .alan:
            return resolveAlan(fileManager: fileManager, environment: environment)
        }
    }

    private static func resolveShell(
        fileManager: FileManager,
        environment: [String: String]
    ) -> AlanCommandResolution {
        let repoRoot = inferredAlanRepoRoot()
        let customCommand = environment["ALAN_SHELL_BOOT_COMMAND"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let shellOverride = normalizedExecutablePath(
            environment["ALAN_SHELL_LOGIN_SHELL"],
            fileManager: fileManager
        )
        let envShell = normalizedExecutablePath(
            environment["SHELL"],
            fileManager: fileManager
        )
        let fallbackCandidates = ["/bin/zsh", "/bin/bash", "/bin/sh"]
        let fallbackShell = fallbackCandidates.first {
            fileManager.isExecutableFile(atPath: $0)
        }

        let candidates = [
            AlanCommandCandidate(
                label: "Env boot command",
                path: customCommand ?? "(unset)",
                isPresent: !(customCommand?.isEmpty ?? true)
            ),
            AlanCommandCandidate(
                label: "Env login shell override",
                path: environment["ALAN_SHELL_LOGIN_SHELL"] ?? "(unset)",
                isPresent: shellOverride != nil
            ),
            AlanCommandCandidate(
                label: "SHELL env",
                path: environment["SHELL"] ?? "(unset)",
                isPresent: envShell != nil
            ),
            AlanCommandCandidate(
                label: "Fallback login shell",
                path: fallbackShell ?? fallbackCandidates.joined(separator: ", "),
                isPresent: fallbackShell != nil
            ),
        ]

        if let customCommand, !customCommand.isEmpty {
            return AlanCommandResolution(
                strategy: .shellCommandEnv,
                executablePath: nil,
                launchPath: "/bin/zsh",
                arguments: ["-lc", customCommand],
                bootCommand: customCommand,
                surfaceCommand: customCommand,
                summary: "Launching pane from ALAN_SHELL_BOOT_COMMAND",
                detail: customCommand,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if let shellOverride {
            return directShell(
                strategy: .loginShellOverride,
                executablePath: shellOverride,
                summary: "Launching pane from ALAN_SHELL_LOGIN_SHELL",
                detail: shellOverride,
                repoRoot: repoRoot,
                candidates: candidates,
                inheritGhosttyCommand: false
            )
        }

        if let envShell {
            return directShell(
                strategy: .loginShellEnv,
                executablePath: envShell,
                summary: "Launching pane from SHELL",
                detail: envShell,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        return directShell(
            strategy: .loginShellFallback,
            executablePath: fallbackShell ?? "/bin/zsh",
            summary: "Launching pane with the default login shell",
            detail: fallbackShell ?? "/bin/zsh",
            repoRoot: repoRoot,
            candidates: candidates
        )
    }

    private static func resolveAlan(
        fileManager: FileManager,
        environment: [String: String]
    ) -> AlanCommandResolution {
        let repoRoot = inferredAlanRepoRoot()
        let channel = AlanInstallChannel.current(environment: environment)
        let cliName = channel.cliToolName

        let envOverride = normalizedExecutablePath(
            environment["ALAN_SHELL_ALAN_PATH"],
            fileManager: fileManager
        )
        let repoDebug = repoRoot.flatMap {
            normalizedExecutablePath("\($0)/target/debug/alan", fileManager: fileManager)
        }
        let repoRelease = repoRoot.flatMap {
            normalizedExecutablePath("\($0)/target/release/alan", fileManager: fileManager)
        }
        let bundled = bundledAlanExecutablePath(
            channel: channel,
            environment: environment,
            fileManager: fileManager
        )
        let pathBinary = searchPath(
            executable: cliName,
            environment: environment,
            fileManager: fileManager
        )

        let candidates = [
            AlanCommandCandidate(
                label: "Env override",
                path: environment["ALAN_SHELL_ALAN_PATH"] ?? "(unset)",
                isPresent: envOverride != nil
            ),
            AlanCommandCandidate(
                label: "Repo debug",
                path: repoRoot.map { "\($0)/target/debug/alan" } ?? "(unknown repo root)",
                isPresent: repoDebug != nil
            ),
            AlanCommandCandidate(
                label: "Repo release",
                path: repoRoot.map { "\($0)/target/release/alan" } ?? "(unknown repo root)",
                isPresent: repoRelease != nil
            ),
            AlanCommandCandidate(
                label: "App bundle",
                path: bundled?.path ?? "(not bundled)",
                isPresent: bundled != nil
            ),
            AlanCommandCandidate(
                label: "PATH lookup",
                path: pathBinary ?? cliName,
                isPresent: pathBinary != nil
            ),
        ]

        if let envOverride {
            return directBinary(
                strategy: .envOverride,
                executablePath: envOverride,
                summary: "Launching alan from ALAN_SHELL_ALAN_PATH",
                detail: envOverride,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if channel == .dev, let bundled {
            return directBinary(
                strategy: .bundledResourceBinary,
                executablePath: bundled.path,
                summary: "Launching \(cliName) from the app bundle",
                detail: bundled.path,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if channel == .dev, let pathBinary {
            return directBinary(
                strategy: .pathBinary,
                executablePath: pathBinary,
                summary: "Launching \(cliName) from the current PATH",
                detail: pathBinary,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if let repoDebug {
            return directBinary(
                strategy: .repoDebugBinary,
                executablePath: repoDebug,
                summary: "Launching alan from this worktree's debug binary",
                detail: repoDebug,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if let repoRelease {
            return directBinary(
                strategy: .repoReleaseBinary,
                executablePath: repoRelease,
                summary: "Launching alan from this worktree's release binary",
                detail: repoRelease,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if let bundled {
            return directBinary(
                strategy: .bundledResourceBinary,
                executablePath: bundled.path,
                summary: "Launching \(cliName) from the app bundle",
                detail: bundled.path,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        if let pathBinary {
            return directBinary(
                strategy: .pathBinary,
                executablePath: pathBinary,
                summary: "Launching \(cliName) from the current PATH",
                detail: pathBinary,
                repoRoot: repoRoot,
                candidates: candidates
            )
        }

        return AlanCommandResolution(
            strategy: .shellLookup,
            executablePath: nil,
            launchPath: "/bin/zsh",
            arguments: ["-lc", "\(cliName) chat"],
            bootCommand: "\(cliName) chat",
            surfaceCommand: "\(cliName) chat",
            summary: "No direct \(cliName) binary found; falling back to shell PATH lookup",
            detail: "Make sure `\(cliName)` is in PATH or set ALAN_SHELL_ALAN_PATH.",
            repoRoot: repoRoot,
            candidates: candidates
        )
    }

    private static func bundledAlanExecutablePath(
        channel: AlanInstallChannel,
        environment: [String: String],
        fileManager: FileManager
    ) -> URL? {
        let resourceRoot = environment["ALAN_APP_RESOURCE_DIR"].map {
            URL(fileURLWithPath: $0, isDirectory: true)
        } ?? Bundle.main.resourceURL
        guard let candidate = resourceRoot?
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(channel.cliToolName)
        else {
            return nil
        }

        return fileManager.isExecutableFile(atPath: candidate.path) ? candidate : nil
    }

    private static func directShell(
        strategy: AlanLaunchStrategy,
        executablePath: String,
        summary: String,
        detail: String?,
        repoRoot: String?,
        candidates: [AlanCommandCandidate],
        inheritGhosttyCommand: Bool = true
    ) -> AlanCommandResolution {
        let arguments = ["-l"]
        let bootCommand = ([executablePath] + arguments)
            .map(AlanShellBootProfile.shellQuoted)
            .joined(separator: " ")

        return AlanCommandResolution(
            strategy: strategy,
            executablePath: executablePath,
            launchPath: executablePath,
            arguments: arguments,
            bootCommand: bootCommand,
            surfaceCommand: inheritGhosttyCommand ? nil : bootCommand,
            summary: summary,
            detail: detail,
            repoRoot: repoRoot,
            candidates: candidates
        )
    }

    private static func directBinary(
        strategy: AlanLaunchStrategy,
        executablePath: String,
        summary: String,
        detail: String?,
        repoRoot: String?,
        candidates: [AlanCommandCandidate]
    ) -> AlanCommandResolution {
        let arguments = ["chat"]
        let bootCommand = ([executablePath] + arguments)
            .map(AlanShellBootProfile.shellQuoted)
            .joined(separator: " ")

        return AlanCommandResolution(
            strategy: strategy,
            executablePath: executablePath,
            launchPath: executablePath,
            arguments: arguments,
            bootCommand: bootCommand,
            surfaceCommand: bootCommand,
            summary: summary,
            detail: detail,
            repoRoot: repoRoot,
            candidates: candidates
        )
    }

    private static func normalizedExecutablePath(
        _ rawPath: String?,
        fileManager: FileManager
    ) -> String? {
        guard let rawPath else { return nil }
        let trimmed = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let expanded = (trimmed as NSString).expandingTildeInPath
        guard fileManager.isExecutableFile(atPath: expanded) || fileManager.fileExists(atPath: expanded) else {
            return nil
        }
        return expanded
    }

    private static func searchPath(
        executable: String,
        environment: [String: String],
        fileManager: FileManager
    ) -> String? {
        let pathEntries = (environment["PATH"] ?? "")
            .split(separator: ":")
            .map(String.init)

        for entry in pathEntries where !entry.isEmpty {
            let candidate = (entry as NSString).appendingPathComponent(executable)
            if fileManager.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }

        return nil
    }
}

struct GhosttyIntegrationStatus: Equatable {
    let frameworkPath: String?
    let resourcesPath: String?
    let terminfoPath: String?
    let candidates: [GhosttyDependencyCandidate]

    var isReady: Bool {
        frameworkPath != nil
    }

    var summary: String {
        if let frameworkPath {
            return "GhosttyKit ready at \(frameworkPath)"
        }

        return "GhosttyKit local link not prepared yet"
    }

    var setupCommand: String {
        "clients/apple/scripts/setup-local-ghosttykit.sh"
    }

    static func discover(fileManager: FileManager = .default) -> GhosttyIntegrationStatus {
        let home = fileManager.homeDirectoryForCurrentUser.path
        let environment = ProcessInfo.processInfo.environment
        let repoRoot = inferredAlanRepoRoot()
        let bundleResourceRoot = Bundle.main.resourceURL?.path

        let frameworkCandidates = [
            (
                "Env override",
                environment["ALAN_GHOSTTYKIT_PATH"]?.trimmingCharacters(in: .whitespacesAndNewlines)
            ),
            ("App bundle", bundleResourceRoot.map { "\($0)/GhosttyKit.xcframework" }),
            ("Local apple client link", repoRoot.map { "\($0)/clients/apple/GhosttyKit.xcframework" }),
            ("Ghostty checkout", "\(home)/Developer/ghostty/macos/GhosttyKit.xcframework"),
        ]

        let resourcesCandidates = [
            (
                "Env override",
                environment["ALAN_GHOSTTY_RESOURCES_DIR"]?.trimmingCharacters(in: .whitespacesAndNewlines)
            ),
            ("App bundle", bundleResourceRoot.map { "\($0)/ghostty-resources" }),
            ("Ghostty zig-out", "\(home)/Developer/ghostty/zig-out/share/ghostty"),
            ("Local apple client link", repoRoot.map { "\($0)/clients/apple/ghostty-resources" }),
        ]

        let terminfoCandidates = [
            (
                "Env override",
                environment["ALAN_GHOSTTY_TERMINFO_DIR"]?.trimmingCharacters(in: .whitespacesAndNewlines)
            ),
            ("App bundle", bundleResourceRoot.map { "\($0)/ghostty-terminfo" }),
            ("Ghostty zig-out", "\(home)/Developer/ghostty/zig-out/share/terminfo"),
            ("Local apple client link", repoRoot.map { "\($0)/clients/apple/ghostty-terminfo" }),
        ]

        let frameworkPath = frameworkCandidates
            .compactMap { candidatePath($0.1, fileManager: fileManager) }
            .first

        let resourcesPath = resourcesCandidates
            .compactMap { candidatePath($0.1, fileManager: fileManager) }
            .first

        let terminfoPath = terminfoCandidates
            .compactMap { candidatePath($0.1, fileManager: fileManager) }
            .first

        let candidates =
            frameworkCandidates.map { GhosttyDependencyCandidate(label: "Framework: \($0.0)", path: $0.1 ?? "(unset)", isPresent: candidatePath($0.1, fileManager: fileManager) != nil) }
            + resourcesCandidates.map { GhosttyDependencyCandidate(label: "Resources: \($0.0)", path: $0.1 ?? "(unset)", isPresent: candidatePath($0.1, fileManager: fileManager) != nil) }
            + terminfoCandidates.map { GhosttyDependencyCandidate(label: "Terminfo: \($0.0)", path: $0.1 ?? "(unset)", isPresent: candidatePath($0.1, fileManager: fileManager) != nil) }

        return GhosttyIntegrationStatus(
            frameworkPath: frameworkPath,
            resourcesPath: resourcesPath,
            terminfoPath: terminfoPath,
            candidates: candidates
        )
    }

    private static func candidatePath(_ rawPath: String?, fileManager: FileManager) -> String? {
        guard let rawPath else { return nil }
        let trimmed = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let expanded = (trimmed as NSString).expandingTildeInPath
        return fileManager.fileExists(atPath: expanded) ? expanded : nil
    }
}

struct AlanShellBootProfile: Equatable {
    let command: AlanCommandResolution
    let workingDirectory: String
    let environment: [String: String]
    let ghostty: GhosttyIntegrationStatus

    func requiresSurfaceRecreation(comparedTo previous: AlanShellBootProfile?) -> Bool {
        guard let previous else { return true }
        // Runtime PWD updates and install-time bundle/resource discovery changes must not
        // respawn a pane. Existing terminals only recreate for logical launch-target changes.
        return surfaceRecreationIdentity != previous.surfaceRecreationIdentity
    }

    private var surfaceRecreationIdentity: SurfaceRecreationIdentity {
        SurfaceRecreationIdentity(
            launchTarget: environment["ALAN_SHELL_LAUNCH_TARGET"]
                ?? environment["ALAN_SHELL_BOOT_MODE"]
        )
    }

    var launchCommandString: String {
        command.launchCommandString
    }

    var bootCommand: String {
        command.bootCommand
    }

    var surfaceCommand: String? {
        command.surfaceCommand
    }

    var environmentPreview: [(key: String, value: String)] {
        environment.keys.sorted().map { ($0, environment[$0] ?? "") }
    }

    static func forPane(_ pane: ShellPane, shellState: ShellStateSnapshot) -> AlanShellBootProfile {
        let cwd =
            pane.cwd
            ?? FileManager.default.homeDirectoryForCurrentUser.path
        let ghostty = GhosttyIntegrationStatus.discover()
        let command = AlanCommandResolution.resolve(for: pane.resolvedLaunchTarget)
        let installChannel = AlanInstallChannel.current()
        let controlPlaneRoot = alanShellControlPlaneRootURL(
            windowID: shellState.windowID,
            channel: installChannel
        )
        let controlPlaneSocket = alanShellControlPlaneSocketURL(
            windowID: shellState.windowID,
            channel: installChannel
        )
        let bindingFile = alanShellBindingFileURL(
            windowID: shellState.windowID,
            paneID: pane.paneID,
            channel: installChannel
        )

        var environment: [String: String] = [
            "ALAN_INSTALL_CHANNEL": installChannel.installChannelID,
            "ALAN_SHELL_SOCKET": controlPlaneSocket.path,
            "ALAN_SHELL_WINDOW_ID": shellState.windowID,
            "ALAN_SHELL_SPACE_ID": pane.spaceID,
            "ALAN_SHELL_TAB_ID": pane.tabID,
            "ALAN_SHELL_PANE_ID": pane.paneID,
            "ALAN_SHELL_CONTENT_ID": pane.terminalContentID,
            "ALAN_SHELL_BOOT_MODE": pane.resolvedLaunchTarget.rawValue,
            "ALAN_SHELL_LAUNCH_TARGET": pane.resolvedLaunchTarget.rawValue,
            "ALAN_SHELL_LAUNCH_STRATEGY": command.strategy.rawValue,
            "ALAN_SHELL_CONTROL_DIR": controlPlaneRoot.path,
            "ALAN_SHELL_BINDING_FILE": bindingFile.path,
            "ALAN_SHELL_STATE_FILE": controlPlaneRoot.appendingPathComponent("state.json").path,
            "ALAN_SHELL_COMMANDS_DIR": controlPlaneRoot.appendingPathComponent("commands").path,
            "ALAN_SHELL_RESULTS_DIR": controlPlaneRoot.appendingPathComponent("results").path,
        ]

        if let executablePath = command.executablePath {
            environment["ALAN_SHELL_EXECUTABLE"] = executablePath
        }

        if let repoRoot = command.repoRoot {
            environment["ALAN_REPOSITORY_ROOT"] = repoRoot
        }

        if let terminfoPath = ghostty.terminfoPath {
            environment["TERMINFO"] = terminfoPath
        }
        environment["TERM_PROGRAM"] = "alan"
        environment["COLORTERM"] = "truecolor"

        return AlanShellBootProfile(
            command: command,
            workingDirectory: cwd,
            environment: environment,
            ghostty: ghostty
        )
    }

    static func shellQuoted(_ value: String) -> String {
        guard !value.isEmpty else { return "''" }
        let escaped = value.replacingOccurrences(of: "'", with: "'\\''")
        return "'\(escaped)'"
    }
}

private struct SurfaceRecreationIdentity: Equatable {
    let launchTarget: String?
}

enum TerminalHostStage: String, Equatable {
    case scaffold
    case viewAttached = "view_attached"
    case windowAttached = "window_attached"
    case focused
}

enum TerminalRendererKind: String, Equatable {
    case scaffold
    case ghosttyLive = "ghostty_live"
}

enum TerminalRendererPhase: String, Equatable {
    case pending
    case libraryReady = "library_ready"
    case appReady = "app_ready"
    case surfaceReady = "surface_ready"
    case firstRefresh = "first_refresh"
    case failed
}

struct TerminalRendererSnapshot: Equatable {
    let kind: TerminalRendererKind
    let phase: TerminalRendererPhase
    let summary: String
    let detail: String?
    let failureReason: String?
    let recentEvents: [String]

    var phaseLabel: String {
        phase.rawValue.replacingOccurrences(of: "_", with: " ")
    }

    static let placeholder = TerminalRendererSnapshot(
        kind: .scaffold,
        phase: .pending,
        summary: "AppKit terminal scaffold is active.",
        detail: nil,
        failureReason: nil,
        recentEvents: []
    )
}

struct TerminalPaneMetadataSnapshot: Equatable {
    let title: String?
    let workingDirectory: String?
    let summary: String?
    let attention: ShellAttentionState
    let processExited: Bool
    let lastCommandExitCode: Int?
    let lastUpdatedAt: Date?
    let activeTaskState: ShellTabActiveTaskState?
    let activity: TerminalActivitySnapshot?
    let clearsActivity: Bool

    init(
        title: String?,
        workingDirectory: String?,
        summary: String?,
        attention: ShellAttentionState,
        processExited: Bool,
        lastCommandExitCode: Int?,
        lastUpdatedAt: Date?,
        activeTaskState: ShellTabActiveTaskState? = .inactive,
        activity: TerminalActivitySnapshot? = nil,
        clearsActivity: Bool = false
    ) {
        self.title = title
        self.workingDirectory = workingDirectory
        self.summary = summary
        self.attention = attention
        self.processExited = processExited
        self.lastCommandExitCode = lastCommandExitCode
        self.lastUpdatedAt = lastUpdatedAt
        self.activeTaskState = activeTaskState
        self.activity = activity
        self.clearsActivity = clearsActivity
    }

    static let placeholder = TerminalPaneMetadataSnapshot(
        title: nil,
        workingDirectory: nil,
        summary: nil,
        attention: .idle,
        processExited: false,
        lastCommandExitCode: nil,
        lastUpdatedAt: nil,
        activeTaskState: .inactive,
        activity: nil,
        clearsActivity: false
    )
}

struct TerminalHostRuntimeSnapshot: Equatable {
    let stage: TerminalHostStage
    let contentID: String?
    let paneID: String?
    let tabID: String?
    let logicalSize: CGSize
    let backingSize: CGSize
    let displayName: String?
    let displayID: String?
    let attachedWindowTitle: String?
    let isFocused: Bool
    let renderer: TerminalRendererSnapshot
    let paneMetadata: TerminalPaneMetadataSnapshot
    let surfaceState: AlanTerminalSurfaceStateSnapshot
    let lastUpdatedAt: Date

    var stageLabel: String {
        stage.rawValue.replacingOccurrences(of: "_", with: " ")
    }

    static let placeholder = TerminalHostRuntimeSnapshot(
        stage: .scaffold,
        contentID: nil,
        paneID: nil,
        tabID: nil,
        logicalSize: .zero,
        backingSize: .zero,
        displayName: nil,
        displayID: nil,
        attachedWindowTitle: nil,
        isFocused: false,
        renderer: .placeholder,
        paneMetadata: .placeholder,
        surfaceState: .placeholder,
        lastUpdatedAt: .now
    )
}
#endif
