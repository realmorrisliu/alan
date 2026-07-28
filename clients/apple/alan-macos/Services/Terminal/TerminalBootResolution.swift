import Foundation

#if os(macOS)
func inferredAlanRepoRoot(from filePath: String = #filePath) -> String? {
    var directory = URL(fileURLWithPath: filePath)
        .standardizedFileURL
        .deletingLastPathComponent()

    while true {
        let parent = directory.deletingLastPathComponent()
        if directory.lastPathComponent == "apple", parent.lastPathComponent == "clients" {
            return parent.deletingLastPathComponent().path
        }
        guard parent.path != directory.path else { return nil }
        directory = parent
    }
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

enum AlanLaunchStrategy: String, Codable, Equatable {
    case shellCommandEnv = "shell_command_env"
    case loginShellOverride = "login_shell_override"
    case loginShellEnv = "login_shell_env"
    case loginShellFallback = "login_shell_fallback"
    case terminalProfileSudoUser = "terminal_profile_sudo_user"
    case terminalProfileSudoRoot = "terminal_profile_sudo_root"
    case terminalProfileManagedUser = "terminal_profile_managed_user"
    case terminalProfileCustomCommand = "terminal_profile_custom_command"
}

private enum ShellCoreTerminalProfileResolutionError: Error {
    case unsupportedStrategy(String)
}

struct AlanTerminalProfileBootMetadata: Equatable {
    let requestedID: String?
    let resolvedID: String?
    let kind: String?
    let title: String?
    let state: TerminalProfileResolutionState
}

struct AlanTerminalBootRequest: Equatable {
    let strategy: AlanLaunchStrategy
    let executablePath: String
    let arguments: [String]
    let workingDirectory: String
    let environment: [String: String]
    let bootCommand: String
    let rendererCompatibilityCommand: String?
    let managedUserAccountName: String?
    let terminalProfile: AlanTerminalProfileBootMetadata?

    var launchCommandString: String {
        if let managedUserAccountName {
            return "managed_user \(AlanShellBootProfile.shellQuoted(managedUserAccountName))"
        }
        return ([executablePath] + arguments).map(AlanShellBootProfile.shellQuoted).joined(separator: " ")
    }
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
    let terminalProfileEnvironment: [String: String]
    let managedUserAccountName: String?

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
        terminalProfileState: TerminalProfileResolutionState = .absent,
        terminalProfileEnvironment: [String: String] = [:],
        managedUserAccountName: String? = nil
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
        self.terminalProfileEnvironment = terminalProfileEnvironment
        self.managedUserAccountName = managedUserAccountName
    }

    var launchCommandString: String {
        if let managedUserAccountName {
            return "managed_user \(AlanShellBootProfile.shellQuoted(managedUserAccountName))"
        }
        return ([launchPath] + arguments).map(AlanShellBootProfile.shellQuoted).joined(separator: " ")
    }

    func bootRequest(
        workingDirectory: String,
        environment: [String: String]
    ) -> AlanTerminalBootRequest {
        AlanTerminalBootRequest(
            strategy: strategy,
            executablePath: launchPath,
            arguments: arguments,
            workingDirectory: workingDirectory,
            environment: environment,
            bootCommand: bootCommand,
            rendererCompatibilityCommand: surfaceCommand,
            managedUserAccountName: managedUserAccountName,
            terminalProfile: terminalProfileBootMetadata(environment: environment)
        )
    }

    private func terminalProfileBootMetadata(
        environment: [String: String]
    ) -> AlanTerminalProfileBootMetadata? {
        guard terminalProfile != nil || terminalProfileState != .absent else {
            return nil
        }
        return AlanTerminalProfileBootMetadata(
            requestedID: environment["ALAN_TERMINAL_PROFILE_REQUESTED_ID"],
            resolvedID: terminalProfile?.id,
            kind: terminalProfile?.launch.kind.rawValue,
            title: terminalProfile?.title,
            state: terminalProfileState
        )
    }

    func reinjectingSudoEnvironment(_ environment: [String: String]) -> AlanCommandResolution {
        guard strategy == .terminalProfileSudoUser || strategy == .terminalProfileSudoRoot else {
            return self
        }
        let assignments = Self.sudoReinjectedEnvironmentAssignments(environment)
        guard !assignments.isEmpty else {
            return self
        }

        let reinjectedArguments = arguments
            + ["/usr/bin/env"]
            + assignments
            + ["/bin/sh", "-lc", Self.sudoLoginShellCommand]
        let reinjectedCommand = ([launchPath] + reinjectedArguments)
            .map(AlanShellBootProfile.shellQuoted)
            .joined(separator: " ")
        return AlanCommandResolution(
            strategy: strategy,
            executablePath: executablePath,
            launchPath: launchPath,
            arguments: reinjectedArguments,
            bootCommand: reinjectedCommand,
            surfaceCommand: reinjectedCommand,
            summary: summary,
            detail: detail,
            repoRoot: repoRoot,
            candidates: candidates,
            terminalProfile: terminalProfile,
            terminalProfileState: terminalProfileState,
            terminalProfileEnvironment: terminalProfileEnvironment,
            managedUserAccountName: managedUserAccountName
        )
    }

    private static let sudoLoginShellCommand = """
    shell="${SHELL:-/bin/zsh}"
    case "${shell##*/}" in
      zsh)
        integration="$GHOSTTY_RESOURCES_DIR/shell-integration/zsh"
        if [ -r "$integration/.zshenv" ]; then
          if [ "${ZDOTDIR+x}" = x ]; then
            export GHOSTTY_ZSH_ZDOTDIR="$ZDOTDIR"
          fi
          export ZDOTDIR="$integration"
        fi
        ;;
      bash)
        integration="$GHOSTTY_RESOURCES_DIR/shell-integration/bash/ghostty.bash"
        if [ -r "$integration" ]; then
          export GHOSTTY_BASH_INJECT="1 --noprofile"
          exec -a -bash "$shell" --rcfile "$integration"
        fi
        ;;
      fish)
        integration="$GHOSTTY_RESOURCES_DIR/shell-integration"
        if [ -r "$integration/fish/vendor_conf.d/ghostty-shell-integration.fish" ]; then
          export XDG_DATA_DIRS="$integration${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
        fi
        ;;
    esac
    exec "$shell" -l
    """

    private static let sudoReinjectedEnvironmentAllowlist: Set<String> = [
        "COLORTERM",
        "GHOSTTY_RESOURCES_DIR",
        "TERMINFO",
        "TERM_PROGRAM",
    ]

    private static func sudoReinjectedEnvironmentAssignments(
        _ environment: [String: String]
    ) -> [String] {
        environment.keys.sorted().compactMap { key in
            guard key.hasPrefix("ALAN_") || sudoReinjectedEnvironmentAllowlist.contains(key),
                  let value = environment[key]
            else {
                return nil
            }
            return "\(key)=\(value)"
        }
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
        let requestedID = terminalProfileReference?.trimmingCharacters(in: .whitespacesAndNewlines)
        let hasExplicitTerminalProfileReference = requestedID?.isEmpty == false
        if let managedProfile = locallyResolvedManagedUserProfile(
            requestedID: requestedID,
            terminalProfiles: terminalProfiles
        ),
           case .managedUser(let unixUser) = managedProfile.launch
        {
            return managedUserProfileCommand(
                managedProfile,
                unixUser: unixUser,
                fileManager: fileManager,
                environment: environment
            )
        }

        do {
            let intent = try ShellCoreFFIAdapter.shared.resolveTerminalLaunchIntent(
                terminalProfileReference: terminalProfileReference,
                terminalProfiles: hasExplicitTerminalProfileReference ? terminalProfiles : nil,
                executablePaths: shellCoreExecutablePaths(fileManager: fileManager, environment: environment),
                environment: shellCoreLaunchEnvironment(environment, fileManager: fileManager)
            )
            guard let resolution = commandResolution(
                from: intent,
                repoRoot: inferredAlanRepoRoot(),
                candidates: shellCoreCandidates(
                    for: intent,
                    fileManager: fileManager,
                    environment: environment
                )
            ) else {
                return shellCoreTerminalProfileFailureResolution(
                    terminalProfileReference: terminalProfileReference,
                    fileManager: fileManager,
                    error: ShellCoreTerminalProfileResolutionError.unsupportedStrategy(intent.strategy)
                )
            }
            return resolution
        } catch {
            // With no explicit profile requested, the resolved launch is just a login shell, which
            // the native resolver computes without shell-core. Fall back to it when shell-core
            // cannot load so default terminals still launch a usable shell instead of an
            // immediately-exiting failure command.
            if !hasExplicitTerminalProfileReference {
                return resolveShell(fileManager: fileManager, environment: environment)
            }
            return shellCoreTerminalProfileFailureResolution(
                terminalProfileReference: terminalProfileReference,
                fileManager: fileManager,
                error: error
            )
        }
    }

    private static func locallyResolvedManagedUserProfile(
        requestedID: String?,
        terminalProfiles: TerminalProfileDocument?
    ) -> TerminalProfileDefinition? {
        guard requestedID?.isEmpty == false else {
            return nil
        }
        let profile = terminalProfiles?.profile(id: requestedID)
        guard case .managedUser = profile?.launch else {
            return nil
        }
        return profile
    }

    private static func managedUserProfileCommand(
        _ profile: TerminalProfileDefinition,
        unixUser: String,
        fileManager: FileManager,
        environment: [String: String]
    ) -> AlanCommandResolution {
        let bootCommand = "managed_user \(AlanShellBootProfile.shellQuoted(unixUser))"
        return AlanCommandResolution(
            strategy: .terminalProfileManagedUser,
            executablePath: nil,
            launchPath: "",
            arguments: [],
            bootCommand: bootCommand,
            surfaceCommand: nil,
            summary: "Launching pane with Managed User \(profile.title)",
            detail: profile.redactedDisplayDetail,
            repoRoot: inferredAlanRepoRoot(),
            candidates: profileCandidates(
                profile,
                executablePath: "managed_user",
                fileManager: fileManager,
                environment: environment
            ),
            terminalProfile: profile,
            terminalProfileState: .resolved,
            managedUserAccountName: unixUser
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

    private static func shellCoreCandidates(
        for intent: ShellCoreTerminalLaunchIntent,
        fileManager: FileManager,
        environment: [String: String]
    ) -> [AlanCommandCandidate] {
        switch AlanLaunchStrategy(rawValue: intent.strategy) {
        case .terminalProfileSudoUser,
             .terminalProfileSudoRoot,
             .terminalProfileManagedUser,
             .terminalProfileCustomCommand:
            if let profile = intent.terminalProfile {
                return profileCandidates(
                    profile,
                    executablePath: intent.executablePath ?? intent.launchPath,
                    fileManager: fileManager,
                    environment: environment
                )
            }
        case .shellCommandEnv, .loginShellOverride, .loginShellEnv, .loginShellFallback, nil:
            break
        }
        return shellResolutionCandidates(fileManager: fileManager, environment: environment)
    }

    private static func commandResolution(
        from intent: ShellCoreTerminalLaunchIntent,
        repoRoot: String?,
        candidates: [AlanCommandCandidate]
    ) -> AlanCommandResolution? {
        guard let strategy = AlanLaunchStrategy(rawValue: intent.strategy) else {
            return nil
        }
        return AlanCommandResolution(
            strategy: strategy,
            executablePath: intent.executablePath,
            launchPath: intent.launchPath,
            arguments: intent.arguments,
            bootCommand: intent.bootCommand,
            surfaceCommand: intent.surfaceCommand,
            summary: intent.summary,
            detail: intent.detail,
            repoRoot: repoRoot,
            candidates: candidates,
            terminalProfile: intent.terminalProfile,
            terminalProfileState: intent.resolvedTerminalProfileState,
            terminalProfileEnvironment: intent.profileEnvironment
        )
    }

    private static func shellCoreTerminalProfileFailureResolution(
        terminalProfileReference: String?,
        fileManager: FileManager,
        error: Error
    ) -> AlanCommandResolution {
        let shellPath = "/bin/sh"
        let message = "alan shell-core terminal profile resolution failed: \(error)"
        let script = "printf '%s\\n' \(AlanShellBootProfile.shellQuoted(message)) >&2; exit 78"
        let arguments = ["-lc", script]
        let bootCommand = ([shellPath] + arguments)
            .map(AlanShellBootProfile.shellQuoted)
            .joined(separator: " ")
        let requestedID = terminalProfileReference?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return AlanCommandResolution(
            strategy: .loginShellFallback,
            executablePath: fileManager.isExecutableFile(atPath: shellPath) ? shellPath : nil,
            launchPath: shellPath,
            arguments: arguments,
            bootCommand: bootCommand,
            surfaceCommand: script,
            summary: "Terminal Profile unavailable",
            detail: message,
            repoRoot: inferredAlanRepoRoot(),
            candidates: [
                AlanCommandCandidate(
                    label: "shell-core terminal profile resolution",
                    path: "shell_core_unavailable",
                    isPresent: false
                ),
                AlanCommandCandidate(
                    label: "Failure shell",
                    path: shellPath,
                    isPresent: fileManager.isExecutableFile(atPath: shellPath)
                ),
            ],
            terminalProfile: nil,
            terminalProfileState: .unavailable(
                requestedID: requestedID?.isEmpty == false ? requestedID! : "default",
                reason: "shell_core_unavailable"
            )
        )
    }

    private static func shellCoreExecutablePaths(
        fileManager: FileManager,
        environment: [String: String]
    ) -> Set<String> {
        var paths = Set<String>()
        for path in ["/usr/bin/sudo", "/bin/zsh", "/bin/bash", "/bin/sh"]
            where fileManager.isExecutableFile(atPath: path)
        {
            paths.insert(path)
        }
        for key in ["ALAN_SHELL_LOGIN_SHELL", "SHELL"] {
            if let path = normalizedExecutablePath(environment[key], fileManager: fileManager) {
                paths.insert(path)
            }
        }
        return paths
    }

    private static func shellCoreLaunchEnvironment(
        _ environment: [String: String],
        fileManager: FileManager
    ) -> [String: String] {
        var values = environment
        for key in ["ALAN_SHELL_LOGIN_SHELL", "SHELL"] {
            if let path = normalizedExecutablePath(environment[key], fileManager: fileManager) {
                values[key] = path
            }
        }
        return values
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

        let candidates = shellResolutionCandidates(fileManager: fileManager, environment: environment)

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

    private static func shellResolutionCandidates(
        fileManager: FileManager,
        environment: [String: String]
    ) -> [AlanCommandCandidate] {
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

        return [
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
        bootRequest.launchCommandString
    }

    var bootCommand: String {
        bootRequest.bootCommand
    }

    var surfaceCommand: String? {
        bootRequest.rendererCompatibilityCommand
    }

    var bootRequest: AlanTerminalBootRequest {
        command.bootRequest(
            workingDirectory: workingDirectory,
            environment: environment
        )
    }

    var environmentPreview: [(key: String, value: String)] {
        environment.keys.sorted().map { ($0, environment[$0] ?? "") }
    }

    /// Returns a copy with a different working directory. The expensive boot
    /// inputs (command resolution, Ghostty discovery, environment) are
    /// cwd-independent, so a `cd` only needs to overlay this one field.
    func withWorkingDirectory(_ workingDirectory: String) -> AlanShellBootProfile {
        AlanShellBootProfile(
            command: command,
            workingDirectory: workingDirectory,
            environment: environment,
            ghostty: ghostty
        )
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
        let resolvedCommand = AlanCommandResolution.resolve(
            for: pane.resolvedLaunchTarget,
            terminalProfileReference: terminalProfileReference,
            terminalProfiles: profileDocument,
            fileManager: fileManager,
            environment: processEnvironment
        )
        let cwd =
            pane.cwd
            ?? resolvedCommand.terminalProfile?.defaultWorkingDirectory
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
            "ALAN_SHELL_LAUNCH_STRATEGY": resolvedCommand.strategy.rawValue,
            "ALAN_TERMINAL_PROFILE_STATE": resolvedCommand.terminalProfileState.environmentValue,
            "ALAN_SHELL_CONTROL_DIR": controlPlaneRoot.path,
            "ALAN_SHELL_BINDING_FILE": bindingFile.path,
            "ALAN_SHELL_STATE_FILE": controlPlaneRoot.appendingPathComponent("state.json").path,
            "ALAN_SHELL_COMMANDS_DIR": controlPlaneRoot.appendingPathComponent("commands").path,
            "ALAN_SHELL_RESULTS_DIR": controlPlaneRoot.appendingPathComponent("results").path,
        ]

        if let terminalProfileReference {
            environment["ALAN_TERMINAL_PROFILE_REQUESTED_ID"] = terminalProfileReference
        }
        if let terminalProfile = resolvedCommand.terminalProfile {
            environment["ALAN_TERMINAL_PROFILE_ID"] = terminalProfile.id
            environment["ALAN_TERMINAL_PROFILE_KIND"] = terminalProfile.launch.kind.rawValue
            environment["ALAN_TERMINAL_PROFILE_TITLE"] = terminalProfile.title
        }
        for (key, value) in resolvedCommand.terminalProfileEnvironment {
            environment[key] = value
        }
        if let managedUserAccountName = resolvedCommand.managedUserAccountName {
            environment["ALAN_MANAGED_USER_ACCOUNT"] = managedUserAccountName
        }

        if let executablePath = resolvedCommand.executablePath {
            environment["ALAN_SHELL_EXECUTABLE"] = executablePath
        }

        if let repoRoot = resolvedCommand.repoRoot {
            environment["ALAN_REPOSITORY_ROOT"] = repoRoot
        }

        if let terminfoPath = ghostty.terminfoPath {
            environment["TERMINFO"] = terminfoPath
        }
        if let resourcesPath = ghostty.resourcesPath {
            environment["GHOSTTY_RESOURCES_DIR"] = resourcesPath
        }
        if environment["TERM"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false {
            environment["TERM"] = "xterm-256color"
        }
        environment["TERM_PROGRAM"] = "alan"
        environment["COLORTERM"] = "truecolor"

        let command = resolvedCommand.reinjectingSudoEnvironment(environment)

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

/// Memoizes `AlanShellBootProfile.forPane` results so the hot terminal
/// callbacks (metadata and runtime projection) do not repeatedly hit the
/// filesystem: Ghostty discovery (`stat` of every candidate path), the
/// terminal-profile document load (disk read + JSON decode), and command
/// resolution (PATH/repo lookup).
///
/// A pane's boot profile is its static launch configuration. It only changes
/// when launch-relevant pane inputs change, so the cache keys on exactly those
/// inputs and ignores high-frequency metadata churn (attention, viewport,
/// activity, process binding, alan binding).
final class AlanShellBootProfileCache {
    struct Key: Hashable {
        let paneID: String
        let tabID: String
        let spaceID: String
        let launchTarget: String
        let terminalProfileID: String?
        let windowID: String

        // `cwd` is deliberately excluded: the expensive resolution (Ghostty
        // discovery, terminal-profile document load, command resolution) does
        // not depend on it. The working directory is applied as a cheap overlay
        // so a `cd` never busts the cache or repeats disk work.
        init(pane: ShellPane, windowID: String) {
            paneID = pane.paneID
            tabID = pane.tabID
            spaceID = pane.spaceID
            launchTarget = pane.resolvedLaunchTarget.rawValue
            terminalProfileID = pane.terminalProfileID
            self.windowID = windowID
        }
    }

    private let compute: (ShellPane, ShellStateSnapshot) -> AlanShellBootProfile
    private var storage: [Key: AlanShellBootProfile] = [:]

    init(
        compute: @escaping (ShellPane, ShellStateSnapshot) -> AlanShellBootProfile = {
            AlanShellBootProfile.forPane($0, shellState: $1)
        }
    ) {
        self.compute = compute
    }

    func profile(for pane: ShellPane, shellState: ShellStateSnapshot) -> AlanShellBootProfile {
        let key = Key(pane: pane, windowID: shellState.windowID)
        let base: AlanShellBootProfile
        if let cached = storage[key] {
            base = cached
        } else {
            base = compute(pane, shellState)
            storage[key] = base
        }
        guard let cwd = pane.cwd, cwd != base.workingDirectory else {
            return base
        }
        return base.withWorkingDirectory(cwd)
    }
}

private struct SurfaceRecreationIdentity: Equatable {
    let launchTarget: String?
    let terminalProfileID: String?
    let launchStrategy: String?
}

#endif
