import Foundation

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
    let terminalProfileState: String?
    let terminalProfileRequestedID: String?
    let terminalProfileID: String?
    let terminalProfileKind: String?
    let terminalProfileTitle: String?
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
        terminalProfileState: String? = nil,
        terminalProfileRequestedID: String? = nil,
        terminalProfileID: String? = nil,
        terminalProfileKind: String? = nil,
        terminalProfileTitle: String? = nil,
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
        self.terminalProfileState = terminalProfileState
        self.terminalProfileRequestedID = terminalProfileRequestedID
        self.terminalProfileID = terminalProfileID
        self.terminalProfileKind = terminalProfileKind
        self.terminalProfileTitle = terminalProfileTitle
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
        case terminalProfileState = "terminal_profile_state"
        case terminalProfileRequestedID = "terminal_profile_requested_id"
        case terminalProfileID = "terminal_profile_id"
        case terminalProfileKind = "terminal_profile_kind"
        case terminalProfileTitle = "terminal_profile_title"
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
