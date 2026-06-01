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
    case terminalProfileSudoUser = "terminal_profile_sudo_user"
    case terminalProfileSudoRoot = "terminal_profile_sudo_root"
    case terminalProfileCustomCommand = "terminal_profile_custom_command"
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
    let terminalProfile: TerminalProfileDefinition?
    let terminalProfileState: TerminalProfileResolutionState

    init(
        strategy: AlanLaunchStrategy,
        executablePath: String?,
        launchPath: String,
        arguments: [String],
        bootCommand: String,
        surfaceCommand: String?,
        summary: String,
        detail: String?,
        repoRoot: String?,
        candidates: [AlanCommandCandidate],
        terminalProfile: TerminalProfileDefinition? = nil,
        terminalProfileState: TerminalProfileResolutionState = .absent
    ) {
        self.strategy = strategy
        self.executablePath = executablePath
        self.launchPath = launchPath
        self.arguments = arguments
        self.bootCommand = bootCommand
        self.surfaceCommand = surfaceCommand
        self.summary = summary
        self.detail = detail
        self.repoRoot = repoRoot
        self.candidates = candidates
        self.terminalProfile = terminalProfile
        self.terminalProfileState = terminalProfileState
    }

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
        }
    }

    static func resolve(
        for launchTarget: ShellLaunchTarget,
        terminalProfileReference: String?,
        terminalProfiles: TerminalProfileDocument?,
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> AlanCommandResolution {
        let overrideCommand = environment["ALAN_SHELL_BOOT_COMMAND"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let overrideShell = environment["ALAN_SHELL_LOGIN_SHELL"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if overrideCommand?.isEmpty == false || overrideShell?.isEmpty == false {
            return resolve(for: launchTarget, fileManager: fileManager, environment: environment)
        }

        let document = terminalProfiles ?? .fallback
        let requestedID = terminalProfileReference?.trimmingCharacters(in: .whitespacesAndNewlines)
        let profile =
            requestedID.flatMap(document.profile(id:))
            ?? (requestedID?.isEmpty == false ? nil : document.defaultProfile)
        guard let profile else {
            let fallback = resolve(for: launchTarget, fileManager: fileManager, environment: environment)
            return fallback.withTerminalProfile(
                nil,
                state: requestedID.map { .missing(requestedID: $0) } ?? .absent
            )
        }

        switch profile.launch {
        case .loginShell:
            return resolve(for: launchTarget, fileManager: fileManager, environment: environment)
                .withTerminalProfile(profile, state: .resolved)
        case .sudoUser(let unixUser):
            return profileCommand(
                profile,
                strategy: .terminalProfileSudoUser,
                executablePath: "/usr/bin/sudo",
                arguments: ["-iu", unixUser],
                fileManager: fileManager,
                environment: environment
            )
        case .sudoRoot:
            return profileCommand(
                profile,
                strategy: .terminalProfileSudoRoot,
                executablePath: "/usr/bin/sudo",
                arguments: ["-i"],
                fileManager: fileManager,
                environment: environment
            )
        case .customCommand(let command):
            let executablePath = "/bin/zsh"
            guard fileManager.isExecutableFile(atPath: executablePath) else {
                let fallback = resolve(for: launchTarget, fileManager: fileManager, environment: environment)
                return fallback.withTerminalProfile(
                    profile,
                    state: .unavailable(requestedID: profile.id, reason: "missing_executable")
                )
            }
            return AlanCommandResolution(
                strategy: .terminalProfileCustomCommand,
                executablePath: executablePath,
                launchPath: executablePath,
                arguments: ["-lc", command],
                bootCommand: command,
                surfaceCommand: command,
                summary: "Launching pane with Terminal Profile \(profile.title)",
                detail: profile.redactedDisplayDetail,
                repoRoot: inferredAlanRepoRoot(),
                candidates: profileCandidates(
                    profile,
                    executablePath: executablePath,
                    fileManager: fileManager,
                    environment: environment
                ),
                terminalProfile: profile,
                terminalProfileState: .resolved
            )
        }
    }

    private func withTerminalProfile(
        _ profile: TerminalProfileDefinition?,
        state: TerminalProfileResolutionState
    ) -> AlanCommandResolution {
        AlanCommandResolution(
            strategy: strategy,
            executablePath: executablePath,
            launchPath: launchPath,
            arguments: arguments,
            bootCommand: bootCommand,
            surfaceCommand: surfaceCommand,
            summary: summary,
            detail: detail,
            repoRoot: repoRoot,
            candidates: candidates,
            terminalProfile: profile,
            terminalProfileState: state
        )
    }

    private static func profileCommand(
        _ profile: TerminalProfileDefinition,
        strategy: AlanLaunchStrategy,
        executablePath: String,
        arguments: [String],
        fileManager: FileManager,
        environment: [String: String]
    ) -> AlanCommandResolution {
        guard fileManager.isExecutableFile(atPath: executablePath) else {
            let fallback = resolve(for: .shell, fileManager: fileManager, environment: environment)
            return fallback.withTerminalProfile(
                profile,
                state: .unavailable(requestedID: profile.id, reason: "missing_executable")
            )
        }
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
            summary: "Launching pane with Terminal Profile \(profile.title)",
            detail: profile.redactedDisplayDetail,
            repoRoot: inferredAlanRepoRoot(),
            candidates: profileCandidates(
                profile,
                executablePath: executablePath,
                fileManager: fileManager,
                environment: environment
            ),
            terminalProfile: profile,
            terminalProfileState: .resolved
        )
    }

    private static func profileCandidates(
        _ profile: TerminalProfileDefinition,
        executablePath: String,
        fileManager: FileManager,
        environment: [String: String]
    ) -> [AlanCommandCandidate] {
        [
            AlanCommandCandidate(
                label: "Terminal Profile",
                path: profile.id,
                isPresent: true
            ),
            AlanCommandCandidate(
                label: "Terminal Profile executable",
                path: executablePath,
                isPresent: fileManager.isExecutableFile(atPath: executablePath)
            ),
            AlanCommandCandidate(
                label: "SHELL env",
                path: environment["SHELL"] ?? "(unset)",
                isPresent: normalizedExecutablePath(environment["SHELL"], fileManager: fileManager) != nil
            ),
        ]
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
                ?? environment["ALAN_SHELL_BOOT_MODE"],
            terminalProfileID: environment["ALAN_TERMINAL_PROFILE_ID"]
                ?? environment["ALAN_TERMINAL_PROFILE_REQUESTED_ID"],
            launchStrategy: environment["ALAN_SHELL_LAUNCH_STRATEGY"]
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

    static func forPane(
        _ pane: ShellPane,
        shellState: ShellStateSnapshot,
        terminalProfiles: TerminalProfileDocument? = nil,
        fileManager: FileManager = .default,
        environment processEnvironment: [String: String] = ProcessInfo.processInfo.environment
    ) -> AlanShellBootProfile {
        let ghostty = GhosttyIntegrationStatus.discover()
        let installChannel = AlanInstallChannel.current()
        let profileDocument = terminalProfiles
            ?? TerminalProfileStore.defaultStore(
                channelApplicationSupportDirectoryName: installChannel.applicationSupportDirectoryName,
                fileManager: fileManager,
                environment: processEnvironment
            ).load().document
        // New panes capture Space defaults into `terminalProfileID` when the shell
        // model creates them. Re-reading mutable Space bindings here can recreate
        // already-mounted panes under a different Unix identity after a Space rebind.
        let terminalProfileReference = pane.terminalProfileID
        let command = AlanCommandResolution.resolve(
            for: pane.resolvedLaunchTarget,
            terminalProfileReference: terminalProfileReference,
            terminalProfiles: profileDocument,
            fileManager: fileManager,
            environment: processEnvironment
        )
        let cwd =
            pane.cwd
            ?? command.terminalProfile?.defaultWorkingDirectory
            ?? fileManager.homeDirectoryForCurrentUser.path
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
            "ALAN_TERMINAL_PROFILE_STATE": command.terminalProfileState.environmentValue,
            "ALAN_SHELL_CONTROL_DIR": controlPlaneRoot.path,
            "ALAN_SHELL_BINDING_FILE": bindingFile.path,
            "ALAN_SHELL_STATE_FILE": controlPlaneRoot.appendingPathComponent("state.json").path,
            "ALAN_SHELL_COMMANDS_DIR": controlPlaneRoot.appendingPathComponent("commands").path,
            "ALAN_SHELL_RESULTS_DIR": controlPlaneRoot.appendingPathComponent("results").path,
        ]

        if let terminalProfileReference {
            environment["ALAN_TERMINAL_PROFILE_REQUESTED_ID"] = terminalProfileReference
        }
        if let terminalProfile = command.terminalProfile {
            environment["ALAN_TERMINAL_PROFILE_ID"] = terminalProfile.id
            environment["ALAN_TERMINAL_PROFILE_KIND"] = terminalProfile.launch.kind.rawValue
            environment["ALAN_TERMINAL_PROFILE_TITLE"] = terminalProfile.title
        }

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
    let terminalProfileID: String?
    let launchStrategy: String?
}

enum TerminalRuntimeRenderPriority: Int, Codable, Equatable, CaseIterable, Comparable {
    case hiddenBackground = 0
    case visibleBackground = 1
    case foregroundInteractive = 2

    static func < (
        lhs: TerminalRuntimeRenderPriority,
        rhs: TerminalRuntimeRenderPriority
    ) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    var isVisible: Bool {
        self != .hiddenBackground
    }

    var isForegroundInteractive: Bool {
        self == .foregroundInteractive
    }
}

enum TerminalRenderRefreshReason: String, Equatable {
    case automatic
    case catchUp = "catch_up"
}

struct TerminalRenderCoordinatorMetrics: Codable, Equatable {
    var wakeupRequests = 0
    var appTicks = 0
    var surfaceRefreshes = 0
    var coalescedSurfaceRefreshes = 0
    var catchUpRefreshes = 0
    var cancelledDrains = 0
    var drainBatches = 0
    var lastDrainBatchSize = 0
    var maxDrainBatchSize = 0
    var lastDrainLatencyMs = 0.0
    var maxDrainLatencyMs = 0.0
    var foregroundInteractiveDrains = 0
    var visibleBackgroundDrains = 0
    var hiddenBackgroundDrains = 0

    mutating func recordDrain(priority: TerminalRuntimeRenderPriority) {
        switch priority {
        case .foregroundInteractive:
            foregroundInteractiveDrains += 1
        case .visibleBackground:
            visibleBackgroundDrains += 1
        case .hiddenBackground:
            hiddenBackgroundDrains += 1
        }
    }

    mutating func recordDrainBatch(size: Int, latencyMs: Double) {
        drainBatches += 1
        lastDrainBatchSize = size
        maxDrainBatchSize = max(maxDrainBatchSize, size)
        lastDrainLatencyMs = latencyMs
        maxDrainLatencyMs = max(maxDrainLatencyMs, latencyMs)
    }
}

protocol TerminalRenderCoordinatedHost: AnyObject {
    var terminalRenderPriority: TerminalRuntimeRenderPriority { get }
    var isRenderCoordinatorTargetAlive: Bool { get }

    func renderCoordinatorDrainAppTick()
    func renderCoordinatorRefreshSurface(reason: TerminalRenderRefreshReason)
}

final class TerminalRenderCoordinator {
    private struct PendingWakeup {
        weak var host: TerminalRenderCoordinatedHost?
        let sequence: Int
        let enqueuedAt: DispatchTime
        var requiresSurfaceRefresh: Bool
        var reason: TerminalRenderRefreshReason
    }

    private let lock = NSLock()
    private let automaticallyDrains: Bool
    private var pendingWakeupsByHost: [ObjectIdentifier: PendingWakeup] = [:]
    private var drainScheduled = false
    private var nextSequence = 0

    private var metrics = TerminalRenderCoordinatorMetrics()

    init(automaticallyDrains: Bool = true) {
        self.automaticallyDrains = automaticallyDrains
    }

    func requestWakeup(
        from host: TerminalRenderCoordinatedHost,
        requiresSurfaceRefresh: Bool = true
    ) {
        enqueueWakeup(
            from: host,
            reason: .automatic,
            requiresSurfaceRefresh: requiresSurfaceRefresh
        )
    }

    func requestCatchUp(from host: TerminalRenderCoordinatedHost) {
        enqueueWakeup(
            from: host,
            reason: .catchUp,
            requiresSurfaceRefresh: true
        )
    }

    func drainPending() {
        let pendingWakeups = takePendingWakeups()
        guard !pendingWakeups.isEmpty else { return }

        let drainStartedAt = DispatchTime.now()
        let maxDrainLatencyMs = pendingWakeups
            .map { latencyMs(from: $0.enqueuedAt, to: drainStartedAt) }
            .max() ?? 0
        updateMetrics { metrics in
            metrics.recordDrainBatch(size: pendingWakeups.count, latencyMs: maxDrainLatencyMs)
        }

        for pending in pendingWakeups {
            guard let host = pending.host,
                  host.isRenderCoordinatorTargetAlive
            else {
                updateMetrics { metrics in
                    metrics.cancelledDrains += 1
                }
                continue
            }

            let priority = host.terminalRenderPriority
            updateMetrics { metrics in
                metrics.appTicks += 1
                metrics.recordDrain(priority: priority)
            }
            host.renderCoordinatorDrainAppTick()

            guard pending.requiresSurfaceRefresh else { continue }
            let shouldRefresh = priority.isVisible || pending.reason == .catchUp
            guard shouldRefresh else {
                updateMetrics { metrics in
                    metrics.coalescedSurfaceRefreshes += 1
                }
                continue
            }

            updateMetrics { metrics in
                metrics.surfaceRefreshes += 1
                if pending.reason == .catchUp {
                    metrics.catchUpRefreshes += 1
                }
            }
            host.renderCoordinatorRefreshSurface(reason: pending.reason)
        }
    }

    func metricsSnapshot() -> TerminalRenderCoordinatorMetrics {
        lock.lock()
        let snapshot = metrics
        lock.unlock()
        return snapshot
    }

    private func enqueueWakeup(
        from host: TerminalRenderCoordinatedHost,
        reason: TerminalRenderRefreshReason,
        requiresSurfaceRefresh: Bool
    ) {
        let shouldScheduleDrain: Bool
        lock.lock()
        metrics.wakeupRequests += 1
        let hostID = ObjectIdentifier(host)
        if var pending = pendingWakeupsByHost[hostID] {
            pending.requiresSurfaceRefresh = pending.requiresSurfaceRefresh || requiresSurfaceRefresh
            if reason == .catchUp {
                pending.reason = .catchUp
            }
            pendingWakeupsByHost[hostID] = pending
        } else {
            pendingWakeupsByHost[hostID] = PendingWakeup(
                host: host,
                sequence: nextSequence,
                enqueuedAt: DispatchTime.now(),
                requiresSurfaceRefresh: requiresSurfaceRefresh,
                reason: reason
            )
            nextSequence += 1
        }
        shouldScheduleDrain = automaticallyDrains && !drainScheduled
        if shouldScheduleDrain {
            drainScheduled = true
        }
        lock.unlock()

        guard shouldScheduleDrain else { return }
        DispatchQueue.main.async { [weak self] in
            self?.drainPending()
        }
    }

    private func takePendingWakeups() -> [PendingWakeup] {
        lock.lock()
        let pending = pendingWakeupsByHost.values.sorted { lhs, rhs in
            let lhsPriority = lhs.host?.terminalRenderPriority ?? .hiddenBackground
            let rhsPriority = rhs.host?.terminalRenderPriority ?? .hiddenBackground
            if lhsPriority != rhsPriority {
                return lhsPriority > rhsPriority
            }
            return lhs.sequence < rhs.sequence
        }
        pendingWakeupsByHost.removeAll()
        drainScheduled = false
        lock.unlock()
        return pending
    }

    private func latencyMs(from start: DispatchTime, to end: DispatchTime) -> Double {
        let nanos = end.uptimeNanoseconds >= start.uptimeNanoseconds
            ? end.uptimeNanoseconds - start.uptimeNanoseconds
            : 0
        return Double(nanos) / 1_000_000
    }

    private func updateMetrics(_ update: (inout TerminalRenderCoordinatorMetrics) -> Void) {
        lock.lock()
        update(&metrics)
        lock.unlock()
    }
}

func terminalRuntimeRenderPriority(
    paneID: String,
    paneSpaceID: String,
    paneTabID: String,
    selectedSpaceID: String?,
    selectedTabID: String?,
    focusedPaneID: String?,
    visiblePaneIDs: Set<String>,
    windowIsVisible: Bool
) -> TerminalRuntimeRenderPriority {
    guard windowIsVisible,
          paneSpaceID == selectedSpaceID,
          paneTabID == selectedTabID,
          visiblePaneIDs.contains(paneID)
    else {
        return .hiddenBackground
    }

    guard paneID == focusedPaneID else {
        return .visibleBackground
    }
    return .foregroundInteractive
}

enum TerminalRuntimePublicationPolicy {
    static func shouldProjectToShell(
        previous: TerminalHostRuntimeSnapshot?,
        next: TerminalHostRuntimeSnapshot
    ) -> Bool {
        guard next.renderPriority == .hiddenBackground else {
            return true
        }
        guard let previous else {
            return true
        }

        return hiddenSummaryChanged(previous: previous, next: next)
    }

    private static func hiddenSummaryChanged(
        previous: TerminalHostRuntimeSnapshot,
        next: TerminalHostRuntimeSnapshot
    ) -> Bool {
        previous.contentID != next.contentID
            || previous.paneID != next.paneID
            || previous.tabID != next.tabID
            || previous.stage != next.stage
            || previous.renderPriority != next.renderPriority
            || previous.renderer.failureReason != next.renderer.failureReason
            || (previous.renderer.phase != .failed && next.renderer.phase == .failed)
            || previous.paneMetadata.title != next.paneMetadata.title
            || previous.paneMetadata.workingDirectory != next.paneMetadata.workingDirectory
            || previous.paneMetadata.summary != next.paneMetadata.summary
            || previous.paneMetadata.attention != next.paneMetadata.attention
            || previous.paneMetadata.processExited != next.paneMetadata.processExited
            || previous.paneMetadata.lastCommandExitCode != next.paneMetadata.lastCommandExitCode
            || previous.paneMetadata.activeTaskState != next.paneMetadata.activeTaskState
            || previous.paneMetadata.activity != next.paneMetadata.activity
            || previous.paneMetadata.clearsActivity != next.paneMetadata.clearsActivity
            || previous.surfaceState.readiness != next.surfaceState.readiness
            || previous.surfaceState.inputReady != next.surfaceState.inputReady
    }
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
    let renderPriority: TerminalRuntimeRenderPriority
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

    init(
        stage: TerminalHostStage,
        contentID: String?,
        paneID: String?,
        tabID: String?,
        renderPriority: TerminalRuntimeRenderPriority = .foregroundInteractive,
        logicalSize: CGSize,
        backingSize: CGSize,
        displayName: String?,
        displayID: String?,
        attachedWindowTitle: String?,
        isFocused: Bool,
        renderer: TerminalRendererSnapshot,
        paneMetadata: TerminalPaneMetadataSnapshot,
        surfaceState: AlanTerminalSurfaceStateSnapshot,
        lastUpdatedAt: Date
    ) {
        self.stage = stage
        self.contentID = contentID
        self.paneID = paneID
        self.tabID = tabID
        self.renderPriority = renderPriority
        self.logicalSize = logicalSize
        self.backingSize = backingSize
        self.displayName = displayName
        self.displayID = displayID
        self.attachedWindowTitle = attachedWindowTitle
        self.isFocused = isFocused
        self.renderer = renderer
        self.paneMetadata = paneMetadata
        self.surfaceState = surfaceState
        self.lastUpdatedAt = lastUpdatedAt
    }

    var stageLabel: String {
        stage.rawValue.replacingOccurrences(of: "_", with: " ")
    }

    static let placeholder = TerminalHostRuntimeSnapshot(
        stage: .scaffold,
        contentID: nil,
        paneID: nil,
        tabID: nil,
        renderPriority: .hiddenBackground,
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
