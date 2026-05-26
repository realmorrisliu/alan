import Foundation

enum ShellAttentionState: String, Codable, CaseIterable {
    case idle
    case active
    case awaitingUser = "awaiting_user"
    case notable
}

enum ShellTabKind: String, Codable, CaseIterable {
    case terminal
    case scratch
    case log
}

enum ShellTabOrganizationSection: String, Codable, CaseIterable {
    case pinned
    case unpinned
}

struct ShellTabOrganizationLocation: Codable, Equatable {
    let spaceID: String
    let section: ShellTabOrganizationSection
    let index: Int

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case section
        case index
    }
}

enum ShellPaneTreeKind: String, Codable {
    case split
    case pane
}

enum ShellSplitDirection: String, Codable {
    case horizontal
    case vertical
}

enum ShellPaneSplitDirection: String, Codable, CaseIterable {
    case left
    case right
    case up
    case down

    var splitDirection: ShellSplitDirection {
        switch self {
        case .left, .right:
            return .vertical
        case .up, .down:
            return .horizontal
        }
    }

    var placesNewPaneBeforeTarget: Bool {
        switch self {
        case .left, .up:
            return true
        case .right, .down:
            return false
        }
    }

    var spatialFocusDirection: ShellSpatialFocusDirection {
        switch self {
        case .left:
            return .left
        case .right:
            return .right
        case .up:
            return .up
        case .down:
            return .down
        }
    }

    static func defaultPlacement(for splitDirection: ShellSplitDirection) -> ShellPaneSplitDirection {
        switch splitDirection {
        case .horizontal:
            return .down
        case .vertical:
            return .right
        }
    }
}

enum ShellSpatialFocusDirection: String, Codable, CaseIterable {
    case left
    case right
    case up
    case down

    var splitDirection: ShellSplitDirection {
        switch self {
        case .left, .right:
            return .vertical
        case .up, .down:
            return .horizontal
        }
    }

    var movesForward: Bool {
        switch self {
        case .right, .down:
            return true
        case .left, .up:
            return false
        }
    }
}

enum ShellWorkspaceCommand: String, CaseIterable, Identifiable {
    case newTerminalTab
    case splitLeft
    case splitRight
    case splitUp
    case splitDown
    case focusLeft
    case focusRight
    case focusUp
    case focusDown
    case equalizeSplits
    case togglePaneZoom
    case movePaneLeft
    case movePaneRight
    case movePaneUp
    case movePaneDown
    case closePane
    case closeTab
    case quickTerminalToggle
    case quickTerminalShow
    case quickTerminalHide
    case quickTerminalFocus
    case quickTerminalClose

    var id: String { rawValue }
}

enum ShellLaunchTarget: String, Codable, CaseIterable {
    case shell
}

enum ShellTabActiveTaskState: String, Codable, Equatable, CaseIterable {
    case inactive
    case foregroundCommand = "foreground_command"
    case alanRunning = "alan_running"
    case alanPendingYield = "alan_pending_yield"
    case alanSession = "alan_session"
    case unknown

    var protectsFromPruning: Bool {
        switch self {
        case .inactive:
            return false
        case .foregroundCommand, .alanRunning, .alanPendingYield, .alanSession, .unknown:
            return true
        }
    }
}

enum TerminalActivitySourceKind: String, Codable, Equatable, CaseIterable {
    case codex
    case claude
    case openCode = "open_code"
    case alan
    case shell
    case progress
    case command
    case process
    case unknown
}

enum TerminalActivityStatus: String, Codable, Equatable, CaseIterable {
    case needsInput = "needs_input"
    case failed
    case paused
    case progress
    case running
    case bell
    case exited
    case idle
    case done
    case stale
}

enum TerminalActivityPriority: String, Codable, Equatable, CaseIterable {
    case passive
    case active
    case notable
    case awaitingUser = "awaiting_user"

    var sidebarPriorityRank: Int {
        switch self {
        case .awaitingUser:
            return 40
        case .notable:
            return 30
        case .active:
            return 20
        case .passive:
            return 10
        }
    }
}

enum TerminalActivityProgressKind: String, Codable, Equatable, CaseIterable {
    case percent
    case indeterminate
    case paused
    case failed
}

struct TerminalActivitySource: Codable, Equatable {
    let kind: TerminalActivitySourceKind
    let label: String?
}

struct TerminalActivityProgress: Codable, Equatable {
    let kind: TerminalActivityProgressKind
    let percent: Int?

    init(kind: TerminalActivityProgressKind, percent: Int? = nil) {
        self.kind = kind
        self.percent = percent.map { min(max($0, 0), 100) }
    }

    static func percent(_ value: Int) -> TerminalActivityProgress {
        TerminalActivityProgress(kind: .percent, percent: value)
    }

    static let indeterminate = TerminalActivityProgress(kind: .indeterminate)
    static let paused = TerminalActivityProgress(kind: .paused)
    static let failed = TerminalActivityProgress(kind: .failed)
}

struct TerminalActivityCommandOutcome: Codable, Equatable {
    let exitCode: Int?
    let durationMilliseconds: Int?
    let commandText: String?

    private enum CodingKeys: String, CodingKey {
        case exitCode = "exit_code"
        case durationMilliseconds = "duration_milliseconds"
        case commandText = "command_text"
    }
}

struct TerminalActivityAgentMetadata: Codable, Equatable {
    let kind: TerminalActivitySourceKind
    let safeSessionLabel: String?
    let projectLabel: String?
    let workingDirectory: String?

    private enum CodingKeys: String, CodingKey {
        case kind
        case safeSessionLabel = "safe_session_label"
        case projectLabel = "project_label"
        case workingDirectory = "working_directory"
    }
}

struct TerminalAgentActivityEvent: Equatable {
    let agentKind: String
    let status: String
    let sessionLabel: String?
    let projectLabel: String?
    let workingDirectory: String?
    let detail: String?
    let updatedAt: String?
}

enum TerminalAgentActivityAdapter {
    private static let iso8601Formatter = ISO8601DateFormatter()

    static func activity(
        from event: TerminalAgentActivityEvent,
        now: Date = Date()
    ) -> TerminalActivitySnapshot? {
        guard let sourceKind = sourceKind(for: event.agentKind) else { return nil }
        guard let status = mappedStatus(for: event.status) else { return nil }

        let updatedAtDate = event.updatedAt.flatMap(iso8601Formatter.date(from:)) ?? now
        let updatedAt = iso8601Formatter.string(from: updatedAtDate)
        let sourceLabel = sourceLabel(for: sourceKind)
        let workingDirectory = sanitizedLabel(event.workingDirectory, maxLength: 240)
        let projectLabel = sanitizedLabel(event.projectLabel, maxLength: 48)
            ?? pathLeaf(from: workingDirectory)

        return TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: sourceKind, label: sourceLabel),
            status: status.status,
            priority: status.priority,
            progress: nil,
            command: nil,
            agent: TerminalActivityAgentMetadata(
                kind: sourceKind,
                safeSessionLabel: sanitizedSessionLabel(event.sessionLabel),
                projectLabel: projectLabel,
                workingDirectory: workingDirectory
            ),
            display: TerminalActivityDisplay(
                sourceLabel: sourceLabel,
                stateLabel: status.stateLabel,
                detailLabel: sanitizedDetail(event.detail),
                paneHint: nil
            ),
            freshness: freshness(for: status.status, updatedAt: updatedAtDate, updatedAtString: updatedAt)
        )
    }

    private struct MappedStatus {
        let status: TerminalActivityStatus
        let priority: TerminalActivityPriority
        let stateLabel: String
    }

    private static func sourceKind(for raw: String) -> TerminalActivitySourceKind? {
        let token = normalizedToken(raw)
        guard !token.isEmpty else { return nil }

        switch token {
        case "codex", "openaicodex":
            return .codex
        default:
            return .unknown
        }
    }

    private static func sourceLabel(for sourceKind: TerminalActivitySourceKind) -> String {
        switch sourceKind {
        case .codex:
            return "Codex"
        default:
            return "Agent"
        }
    }

    private static func mappedStatus(for raw: String) -> MappedStatus? {
        switch normalizedToken(raw) {
        case "running", "inprogress", "working", "thinking", "streaming", "toolrunning":
            return MappedStatus(status: .running, priority: .active, stateLabel: "Running")
        case "needsinput", "inputrequired", "waitingforinput", "requiresinput",
             "approvalrequired", "requiresapproval", "blocked":
            return MappedStatus(status: .needsInput, priority: .awaitingUser, stateLabel: "Input needed")
        case "completed", "complete", "done", "success", "succeeded", "idle":
            return MappedStatus(status: .done, priority: .passive, stateLabel: "Done")
        case "failed", "failure", "error", "errored", "cancelled", "canceled":
            return MappedStatus(status: .failed, priority: .notable, stateLabel: "Error")
        case "paused":
            return MappedStatus(status: .paused, priority: .active, stateLabel: "Paused")
        default:
            return nil
        }
    }

    private static func freshness(
        for status: TerminalActivityStatus,
        updatedAt: Date,
        updatedAtString: String
    ) -> TerminalActivityFreshness {
        switch status {
        case .running:
            return TerminalActivityFreshness(
                updatedAt: updatedAtString,
                staleAt: iso8601Formatter.string(from: updatedAt.addingTimeInterval(90)),
                expiresAt: nil
            )
        case .done:
            return TerminalActivityFreshness(
                updatedAt: updatedAtString,
                staleAt: nil,
                expiresAt: iso8601Formatter.string(from: updatedAt.addingTimeInterval(8))
            )
        case .needsInput, .failed:
            return TerminalActivityFreshness(updatedAt: updatedAtString, staleAt: nil, expiresAt: nil)
        case .paused:
            return TerminalActivityFreshness(
                updatedAt: updatedAtString,
                staleAt: iso8601Formatter.string(from: updatedAt.addingTimeInterval(90)),
                expiresAt: nil
            )
        case .progress, .bell, .exited, .idle, .stale:
            return TerminalActivityFreshness(updatedAt: updatedAtString, staleAt: nil, expiresAt: nil)
        }
    }

    private static func sanitizedSessionLabel(_ raw: String?) -> String? {
        guard let label = sanitizedLabel(raw, maxLength: 32) else { return nil }
        let lowercased = label.lowercased()
        guard !lowercased.contains("session"),
              !lowercased.hasPrefix("sess"),
              !looksLikeRawIdentifier(label)
        else {
            return nil
        }
        return label
    }

    private static func sanitizedDetail(_ raw: String?) -> String? {
        guard let detail = sanitizedLabel(raw, maxLength: 80) else { return nil }
        let lowercased = detail.lowercased()
        guard !detail.hasPrefix("{"),
              !detail.hasPrefix("["),
              !lowercased.contains("event"),
              !lowercased.contains("session_id")
        else {
            return nil
        }
        return detail
    }

    private static func sanitizedLabel(_ raw: String?, maxLength: Int) -> String? {
        guard let raw else { return nil }
        let cleaned = raw.unicodeScalars
            .map { scalar in
                CharacterSet.controlCharacters.contains(scalar) ? " " : String(scalar)
            }
            .joined()
        let collapsed = cleaned
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        guard !collapsed.isEmpty else { return nil }

        let clipped = String(collapsed.prefix(maxLength))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return clipped.isEmpty ? nil : clipped
    }

    private static func pathLeaf(from raw: String?) -> String? {
        guard let raw, raw.contains("/") else { return nil }
        let leaf = URL(fileURLWithPath: raw).lastPathComponent
        return sanitizedLabel(leaf, maxLength: 48)
    }

    private static func looksLikeRawIdentifier(_ label: String) -> Bool {
        let alphanumericCount = label.filter { $0.isLetter || $0.isNumber }.count
        guard alphanumericCount >= 20 else { return false }
        let hexCount = label.filter(\.isHexDigit).count
        return Double(hexCount) / Double(alphanumericCount) > 0.7
    }

    private static func normalizedToken(_ raw: String) -> String {
        raw.lowercased().filter { $0.isLetter || $0.isNumber }
    }
}

struct TerminalActivityDisplay: Codable, Equatable {
    let sourceLabel: String
    let stateLabel: String
    let detailLabel: String?
    let paneHint: String?

    var sourceFirstLabel: String {
        [paneHint, "\(sourceLabel) · \(stateLabel)"]
            .compactMap { label -> String? in
                guard let label, !label.isEmpty else { return nil }
                return label
            }
            .joined(separator: " · ")
    }

    private enum CodingKeys: String, CodingKey {
        case sourceLabel = "source_label"
        case stateLabel = "state_label"
        case detailLabel = "detail_label"
        case paneHint = "pane_hint"
    }
}

struct TerminalActivityFreshness: Codable, Equatable {
    let updatedAt: String
    let staleAt: String?
    let expiresAt: String?

    private enum CodingKeys: String, CodingKey {
        case updatedAt = "updated_at"
        case staleAt = "stale_at"
        case expiresAt = "expires_at"
    }
}

struct TerminalActivitySnapshot: Codable, Equatable {
    private static let iso8601Formatter = ISO8601DateFormatter()

    let source: TerminalActivitySource
    let status: TerminalActivityStatus
    let priority: TerminalActivityPriority
    let progress: TerminalActivityProgress?
    let command: TerminalActivityCommandOutcome?
    let agent: TerminalActivityAgentMetadata?
    let display: TerminalActivityDisplay
    let freshness: TerminalActivityFreshness

    var isCommandFailure: Bool {
        source.kind == .command && status == .failed
    }

    var isSidebarWorthy: Bool {
        isSidebarWorthy(at: nil, owningTabFocused: false)
    }

    func isSidebarWorthy(at now: Date?, owningTabFocused: Bool = false) -> Bool {
        if let now, !isFresh(at: now) {
            return false
        }
        if owningTabFocused,
           isCommandFailure
        {
            return false
        }

        switch status {
        case .needsInput, .failed, .paused, .progress, .running, .bell, .exited:
            return true
        case .idle, .done, .stale:
            return false
        }
    }

    var sidebarPriorityRank: Int {
        switch status {
        case .needsInput:
            return 70
        case .failed:
            return 60
        case .paused:
            return 50
        case .progress:
            return 40
        case .running:
            return 30
        case .bell, .exited:
            return 20
        case .idle, .done, .stale:
            return 0
        }
    }

    func isFresh(at now: Date) -> Bool {
        if let expiresAt = freshness.expiresAt.flatMap(Self.iso8601Formatter.date(from:)),
           now >= expiresAt
        {
            return false
        }

        if let staleAt = freshness.staleAt.flatMap(Self.iso8601Formatter.date(from:)),
           now >= staleAt
        {
            return false
        }

        return true
    }

    func withPaneHint(_ paneHint: String?) -> TerminalActivitySnapshot {
        TerminalActivitySnapshot(
            source: source,
            status: status,
            priority: priority,
            progress: progress,
            command: command,
            agent: agent,
            display: TerminalActivityDisplay(
                sourceLabel: display.sourceLabel,
                stateLabel: display.stateLabel,
                detailLabel: display.detailLabel,
                paneHint: paneHint
            ),
            freshness: freshness
        )
    }

    static func primarySidebarActivity(
        _ activities: [TerminalActivitySnapshot]
    ) -> TerminalActivitySnapshot? {
        primarySidebarActivity(activities, now: Date())
    }

    static func primarySidebarActivity(
        _ activities: [TerminalActivitySnapshot],
        now: Date?
    ) -> TerminalActivitySnapshot? {
        activities
            .filter { activity in
                activity.isSidebarWorthy(at: now)
            }
            .max { lhs, rhs in
                if lhs.sidebarPriorityRank == rhs.sidebarPriorityRank {
                    if lhs.priority.sidebarPriorityRank == rhs.priority.sidebarPriorityRank {
                        if lhs.freshness.updatedAt == rhs.freshness.updatedAt {
                            return lhs.source.kind.rawValue < rhs.source.kind.rawValue
                        }
                        return lhs.freshness.updatedAt < rhs.freshness.updatedAt
                    }
                    return lhs.priority.sidebarPriorityRank < rhs.priority.sidebarPriorityRank
                }
                return lhs.sidebarPriorityRank < rhs.sidebarPriorityRank
            }
    }

    static func progressActivity(percent: Int, now: Date) -> TerminalActivitySnapshot {
        let boundedPercent = min(max(percent, 0), 100)
        return progressActivity(
            progress: .percent(boundedPercent),
            status: .progress,
            priority: .active,
            stateLabel: "\(boundedPercent)%",
            now: now
        )
    }

    static func progressActivity(
        progress: TerminalActivityProgress,
        status: TerminalActivityStatus,
        priority: TerminalActivityPriority,
        stateLabel: String,
        now: Date
    ) -> TerminalActivitySnapshot {
        return TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: .progress, label: "Progress"),
            status: status,
            priority: priority,
            progress: progress,
            command: nil,
            agent: nil,
            display: TerminalActivityDisplay(
                sourceLabel: "Progress",
                stateLabel: stateLabel,
                detailLabel: nil,
                paneHint: nil
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: Self.iso8601Formatter.string(from: now),
                staleAt: Self.iso8601Formatter.string(from: now.addingTimeInterval(15)),
                expiresAt: nil
            )
        )
    }

    static func commandCompletion(
        exitCode: Int,
        now: Date,
        durationMilliseconds: Int? = nil
    ) -> TerminalActivitySnapshot {
        let succeeded = exitCode == 0
        let status: TerminalActivityStatus = succeeded ? .done : .failed
        let priority: TerminalActivityPriority = succeeded ? .passive : .notable
        let stateLabel = succeeded ? "Command succeeded" : "Command failed \(exitCode)"
        return TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: .command, label: "Shell"),
            status: status,
            priority: priority,
            progress: nil,
            command: TerminalActivityCommandOutcome(
                exitCode: exitCode,
                durationMilliseconds: durationMilliseconds,
                commandText: nil
            ),
            agent: nil,
            display: TerminalActivityDisplay(
                sourceLabel: "Shell",
                stateLabel: stateLabel,
                detailLabel: nil,
                paneHint: nil
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: Self.iso8601Formatter.string(from: now),
                staleAt: Self.iso8601Formatter.string(from: now.addingTimeInterval(succeeded ? 8 : 30)),
                expiresAt: nil
            )
        )
    }

    static func bellActivity(now: Date) -> TerminalActivitySnapshot {
        TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: .shell, label: "Shell"),
            status: .bell,
            priority: .active,
            progress: nil,
            command: nil,
            agent: nil,
            display: TerminalActivityDisplay(
                sourceLabel: "Shell",
                stateLabel: "Bell",
                detailLabel: nil,
                paneHint: nil
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: Self.iso8601Formatter.string(from: now),
                staleAt: nil,
                expiresAt: Self.iso8601Formatter.string(from: now.addingTimeInterval(8))
            )
        )
    }

    static func processExitedActivity(exitCode: Int?, now: Date) -> TerminalActivitySnapshot {
        let stateLabel = exitCode.map { "Exited \($0)" } ?? "Exited"
        return TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: .process, label: "Process"),
            status: .exited,
            priority: .notable,
            progress: nil,
            command: TerminalActivityCommandOutcome(
                exitCode: exitCode,
                durationMilliseconds: nil,
                commandText: nil
            ),
            agent: nil,
            display: TerminalActivityDisplay(
                sourceLabel: "Process",
                stateLabel: stateLabel,
                detailLabel: nil,
                paneHint: nil
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: Self.iso8601Formatter.string(from: now),
                staleAt: nil,
                expiresAt: nil
            )
        )
    }
}

struct ShellProcessBinding: Codable, Equatable {
    let program: String
    let argvPreview: [String]?

    private enum CodingKeys: String, CodingKey {
        case program
        case argvPreview = "argv_preview"
    }
}

struct ShellContextSnapshot: Codable, Equatable {
    let workingDirectoryName: String?
    let repositoryRoot: String?
    let gitBranch: String?
    let controlPath: String?
    let socketPath: String?
    let alanBindingFile: String?
    let launchCommand: String?
    let launchStrategy: String?
    let shellIntegrationSource: String?
    let processState: String?
    let rendererPhase: String?
    let rendererHealth: String?
    let surfaceReadiness: String?
    let inputReady: Bool?
    let readonly: Bool?
    let terminalMode: String?
    let displayName: String?
    let displayID: String?
    let windowTitle: String?
    let lastMetadataAt: String?
    let lastCommandExitCode: Int?

    init(
        workingDirectoryName: String?,
        repositoryRoot: String?,
        gitBranch: String?,
        controlPath: String?,
        socketPath: String? = nil,
        alanBindingFile: String?,
        launchCommand: String? = nil,
        launchStrategy: String?,
        shellIntegrationSource: String?,
        processState: String?,
        rendererPhase: String? = nil,
        rendererHealth: String? = nil,
        surfaceReadiness: String? = nil,
        inputReady: Bool? = nil,
        readonly: Bool? = nil,
        terminalMode: String? = nil,
        displayName: String? = nil,
        displayID: String? = nil,
        windowTitle: String? = nil,
        lastMetadataAt: String?,
        lastCommandExitCode: Int?
    ) {
        self.workingDirectoryName = workingDirectoryName
        self.repositoryRoot = repositoryRoot
        self.gitBranch = gitBranch
        self.controlPath = controlPath
        self.socketPath = socketPath
        self.alanBindingFile = alanBindingFile
        self.launchCommand = launchCommand
        self.launchStrategy = launchStrategy
        self.shellIntegrationSource = shellIntegrationSource
        self.processState = processState
        self.rendererPhase = rendererPhase
        self.rendererHealth = rendererHealth
        self.surfaceReadiness = surfaceReadiness
        self.inputReady = inputReady
        self.readonly = readonly
        self.terminalMode = terminalMode
        self.displayName = displayName
        self.displayID = displayID
        self.windowTitle = windowTitle
        self.lastMetadataAt = lastMetadataAt
        self.lastCommandExitCode = lastCommandExitCode
    }

    private enum CodingKeys: String, CodingKey {
        case workingDirectoryName = "working_directory_name"
        case repositoryRoot = "repository_root"
        case gitBranch = "git_branch"
        case controlPath = "control_path"
        case socketPath = "socket_path"
        case alanBindingFile = "alan_binding_file"
        case launchCommand = "launch_command"
        case launchStrategy = "launch_strategy"
        case shellIntegrationSource = "shell_integration_source"
        case processState = "process_state"
        case rendererPhase = "renderer_phase"
        case rendererHealth = "renderer_health"
        case surfaceReadiness = "surface_readiness"
        case inputReady = "input_ready"
        case readonly
        case terminalMode = "terminal_mode"
        case displayName = "display_name"
        case displayID = "display_id"
        case windowTitle = "window_title"
        case lastMetadataAt = "last_metadata_at"
        case lastCommandExitCode = "last_command_exit_code"
    }
}
