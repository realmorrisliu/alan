import Foundation

extension ShellCoreFFIAdapter {
    func validateTerminalProfileDocument(
        _ document: TerminalProfileDocument
    ) throws -> TerminalProfileValidationResult {
        let response: ShellCoreTerminalProfileValidationResponse = try send(
            operation: "terminal_profile.validate",
            payload: document
        )
        return response.validationResult
    }

    func makeTerminalProfileDefinition(
        from draft: TerminalProfileEditorDraft
    ) throws -> TerminalProfileEditorResult {
        let response: ShellCoreTerminalProfileEditorResponse = try send(
            operation: "terminal_profile.make_definition",
            payload: ShellCoreTerminalProfileEditorDraft(draft)
        )
        return response.editorResult
    }

    func resolveTerminalLaunchIntent(
        terminalProfileReference: String?,
        terminalProfiles: TerminalProfileDocument?,
        executablePaths: Set<String>,
        environment: [String: String]
    ) throws -> ShellCoreTerminalLaunchIntent {
        let response: ShellCoreTerminalLaunchIntentResponse = try send(
            operation: "terminal_profile.resolve_launch_intent",
            payload: ShellCoreTerminalLaunchIntentPayload(
                terminalProfileReference: terminalProfileReference,
                terminalProfiles: terminalProfiles,
                executablePaths: executablePaths,
                environment: environment
            )
        )
        return response.intent
    }

}

private struct ShellCoreTerminalProfileValidationResponse: Decodable {
    let isValid: Bool
    let errors: [ShellCoreTerminalProfileValidationError]

    private enum CodingKeys: String, CodingKey {
        case isValid = "is_valid"
        case errors
    }

    var validationResult: TerminalProfileValidationResult {
        TerminalProfileValidationResult(errors: errors.map(\.swiftError))
    }
}

private struct ShellCoreTerminalProfileEditorResponse: Decodable {
    let isValid: Bool
    let definition: TerminalProfileDefinition?
    let errors: [ShellCoreTerminalProfileValidationError]

    private enum CodingKeys: String, CodingKey {
        case isValid = "is_valid"
        case definition
        case errors
    }

    var editorResult: TerminalProfileEditorResult {
        TerminalProfileEditorResult(
            definition: isValid ? definition : nil,
            errors: errors.map(\.swiftError)
        )
    }
}

private struct ShellCoreTerminalProfileEditorDraft: Encodable {
    let id: String
    let title: String
    let launchKind: TerminalProfileLaunchKind
    let unixUser: String
    let customCommand: String
    let defaultWorkingDirectory: String?
    let presentation: TerminalProfilePresentation?
    let managedTerminalAccountID: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case title
        case launchKind = "launch_kind"
        case unixUser = "unix_user"
        case customCommand = "custom_command"
        case defaultWorkingDirectory = "default_working_directory"
        case presentation
        case managedTerminalAccountID = "managed_terminal_account_id"
    }

    init(_ draft: TerminalProfileEditorDraft) {
        id = draft.id
        title = draft.title
        launchKind = draft.launchKind
        unixUser = draft.unixUser
        customCommand = draft.customCommand
        defaultWorkingDirectory = draft.defaultWorkingDirectory
        presentation = draft.presentation
        managedTerminalAccountID = draft.managedTerminalAccountID
    }
}

struct ShellCoreTerminalLaunchIntent: Decodable {
    let strategy: String
    let executablePath: String?
    let launchPath: String
    let arguments: [String]
    let bootCommand: String
    let surfaceCommand: String?
    let summary: String
    let detail: String?
    let terminalProfile: TerminalProfileDefinition?
    let workingDirectory: String?
    let profileEnvironment: [String: String]
    private let terminalProfileState: ShellCoreTerminalProfileResolutionState

    private enum CodingKeys: String, CodingKey {
        case strategy
        case executablePath = "executable_path"
        case launchPath = "launch_path"
        case arguments
        case bootCommand = "boot_command"
        case surfaceCommand = "surface_command"
        case summary
        case detail
        case terminalProfile = "terminal_profile"
        case terminalProfileState = "terminal_profile_state"
        case workingDirectory = "working_directory"
        case profileEnvironment = "profile_environment"
    }

    var resolvedTerminalProfileState: TerminalProfileResolutionState {
        terminalProfileState.swiftState
    }

}

private struct ShellCoreTerminalLaunchIntentResponse: Decodable {
    let intent: ShellCoreTerminalLaunchIntent
}

private struct ShellCoreTerminalLaunchIntentPayload: Encodable {
    let terminalProfileReference: String?
    let terminalProfiles: TerminalProfileDocument?
    let availability: ShellCoreTerminalExecutableAvailabilityPayload
    let environment: ShellCoreTerminalLaunchEnvironmentPayload

    private enum CodingKeys: String, CodingKey {
        case terminalProfileReference = "terminal_profile_reference"
        case terminalProfiles = "terminal_profiles"
        case availability
        case environment
    }

    init(
        terminalProfileReference: String?,
        terminalProfiles: TerminalProfileDocument?,
        executablePaths: Set<String>,
        environment: [String: String]
    ) {
        self.terminalProfileReference = terminalProfileReference
        self.terminalProfiles = terminalProfiles
        availability = ShellCoreTerminalExecutableAvailabilityPayload(executablePaths: executablePaths)
        self.environment = ShellCoreTerminalLaunchEnvironmentPayload(values: environment)
    }
}

private struct ShellCoreTerminalExecutableAvailabilityPayload: Encodable {
    let executablePaths: [String]
    let enforce: Bool

    private enum CodingKeys: String, CodingKey {
        case executablePaths = "executable_paths"
        case enforce
    }

    init(executablePaths: Set<String>) {
        self.executablePaths = executablePaths.sorted()
        enforce = true
    }
}

private struct ShellCoreTerminalLaunchEnvironmentPayload: Encodable {
    let values: [String: String]
}

private enum ShellCoreTerminalProfileResolutionState: Decodable {
    case absent
    case resolved
    case missing(requestedID: String)
    case unavailable(requestedID: String, reason: String)

    private enum CodingKeys: String, CodingKey {
        case state
        case requestedID = "requested_id"
        case reason
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .state) {
        case "absent":
            self = .absent
        case "resolved":
            self = .resolved
        case "missing":
            self = .missing(requestedID: try container.decode(String.self, forKey: .requestedID))
        case "unavailable":
            self = .unavailable(
                requestedID: try container.decode(String.self, forKey: .requestedID),
                reason: try container.decode(String.self, forKey: .reason)
            )
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported Terminal Profile resolution state"
                )
            )
        }
    }

    var swiftState: TerminalProfileResolutionState {
        switch self {
        case .absent:
            return .absent
        case .resolved:
            return .resolved
        case .missing(let requestedID):
            return .missing(requestedID: requestedID)
        case .unavailable(let requestedID, let reason):
            return .unavailable(requestedID: requestedID, reason: reason)
        }
    }
}

private enum ShellCoreTerminalProfileValidationError: Decodable {
    case missingID
    case duplicateID(String)
    case missingTitle(String)
    case missingUnixUser(String)
    case missingCustomCommand(String)
    case missingDefaultProfile(String)
    case unavailableExecutable(profileID: String, path: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case id
        case profileID = "profile_id"
        case path
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "missing_id":
            self = .missingID
        case "duplicate_id":
            self = .duplicateID(try container.decode(String.self, forKey: .id))
        case "missing_title":
            self = .missingTitle(try container.decode(String.self, forKey: .id))
        case "missing_unix_user":
            self = .missingUnixUser(try container.decode(String.self, forKey: .id))
        case "missing_custom_command":
            self = .missingCustomCommand(try container.decode(String.self, forKey: .id))
        case "missing_default_profile":
            self = .missingDefaultProfile(try container.decode(String.self, forKey: .id))
        case "unavailable_executable":
            self = .unavailableExecutable(
                profileID: try container.decode(String.self, forKey: .profileID),
                path: try container.decode(String.self, forKey: .path)
            )
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported Terminal Profile validation error variant"
                )
            )
        }
    }

    var swiftError: TerminalProfileValidationError {
        switch self {
        case .missingID:
            return .missingID
        case .duplicateID(let id):
            return .duplicateID(id)
        case .missingTitle(let id):
            return .missingTitle(id)
        case .missingUnixUser(let id):
            return .missingUnixUser(id)
        case .missingCustomCommand(let id):
            return .missingCustomCommand(id)
        case .missingDefaultProfile(let id):
            return .missingDefaultProfile(id)
        case .unavailableExecutable(let profileID, let path):
            return .unavailableExecutable(profileID: profileID, path: path)
        }
    }
}
