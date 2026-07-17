import Foundation

enum TerminalProfileLaunchKind: String, Codable, CaseIterable {
    case loginShell = "login_shell"
    case sudoUser = "sudo_user"
    case sudoRoot = "sudo_root"
    case managedUser = "managed_user"
    case customCommand = "custom_command"
}

enum TerminalProfileLaunch: Codable, Equatable {
    case loginShell
    case sudoUser(unixUser: String)
    case sudoRoot
    case managedUser(unixUser: String)
    case customCommand(String)

    var kind: TerminalProfileLaunchKind {
        switch self {
        case .loginShell:
            return .loginShell
        case .sudoUser:
            return .sudoUser
        case .sudoRoot:
            return .sudoRoot
        case .managedUser:
            return .managedUser
        case .customCommand:
            return .customCommand
        }
    }

    var unixUser: String? {
        switch self {
        case .sudoUser(let unixUser), .managedUser(let unixUser):
            return unixUser
        case .loginShell, .sudoRoot, .customCommand:
            return nil
        }
    }

    var customCommand: String? {
        guard case .customCommand(let command) = self else { return nil }
        return command
    }

    private enum CodingKeys: String, CodingKey {
        case kind
        case unixUser = "unix_user"
        case command
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(TerminalProfileLaunchKind.self, forKey: .kind) {
        case .loginShell:
            self = .loginShell
        case .sudoUser:
            self = .sudoUser(
                unixUser: try container.decodeIfPresent(String.self, forKey: .unixUser) ?? ""
            )
        case .sudoRoot:
            self = .sudoRoot
        case .managedUser:
            self = .managedUser(
                unixUser: try container.decodeIfPresent(String.self, forKey: .unixUser) ?? ""
            )
        case .customCommand:
            self = .customCommand(
                try container.decodeIfPresent(String.self, forKey: .command) ?? ""
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(kind, forKey: .kind)
        switch self {
        case .loginShell, .sudoRoot:
            break
        case .sudoUser(let unixUser), .managedUser(let unixUser):
            try container.encode(unixUser, forKey: .unixUser)
        case .customCommand(let command):
            try container.encode(command, forKey: .command)
        }
    }
}

struct TerminalProfilePresentation: Codable, Equatable {
    let symbolName: String?
    let colorName: String?

    private enum CodingKeys: String, CodingKey {
        case symbolName = "symbol_name"
        case colorName = "color_name"
    }
}

struct TerminalProfileDefinition: Codable, Equatable, Identifiable {
    let id: String
    let title: String
    let launch: TerminalProfileLaunch
    let defaultWorkingDirectory: String?
    let presentation: TerminalProfilePresentation?
    let managedTerminalAccountID: String?

    init(
        id: String,
        title: String,
        launch: TerminalProfileLaunch,
        defaultWorkingDirectory: String?,
        presentation: TerminalProfilePresentation?,
        managedTerminalAccountID: String? = nil
    ) {
        self.id = id
        self.title = title
        self.launch = launch
        self.defaultWorkingDirectory = defaultWorkingDirectory
        self.presentation = presentation
        self.managedTerminalAccountID = managedTerminalAccountID
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case title
        case launch
        case defaultWorkingDirectory = "default_working_directory"
        case presentation
        case managedTerminalAccountID = "managed_terminal_account_id"
    }

    static let loginShellFallback = TerminalProfileDefinition(
        id: "login_shell",
        title: "Login shell",
        launch: .loginShell,
        defaultWorkingDirectory: nil,
        presentation: TerminalProfilePresentation(symbolName: "terminal", colorName: nil)
    )

    var redactedDisplayDetail: String {
        switch launch {
        case .loginShell:
            return "Login shell"
        case .sudoUser(let unixUser):
            return unixUser.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "Sudo user"
                : "Sudo user \(unixUser)"
        case .sudoRoot:
            return "Root shell"
        case .managedUser(let unixUser):
            return unixUser.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "Managed user"
                : "Managed user \(unixUser)"
        case .customCommand:
            return "Custom command"
        }
    }
}

struct TerminalProfileDocument: Codable, Equatable {
    var defaultProfileID: String
    var profiles: [TerminalProfileDefinition]

    private enum CodingKeys: String, CodingKey {
        case defaultProfileID = "default_profile_id"
        case profiles
    }

    static let fallback = TerminalProfileDocument(
        defaultProfileID: TerminalProfileDefinition.loginShellFallback.id,
        profiles: [TerminalProfileDefinition.loginShellFallback]
    )

    func profile(id: String?) -> TerminalProfileDefinition? {
        guard let id else { return nil }
        return profiles.first { $0.id == id }
    }

    var defaultProfile: TerminalProfileDefinition? {
        profile(id: defaultProfileID) ?? profiles.first
    }
}

enum TerminalProfileValidationError: Error, Equatable {
    case missingID
    case duplicateID(String)
    case missingTitle(String)
    case missingUnixUser(String)
    case missingCustomCommand(String)
    case missingManagedAccount(String)
    case managedAccountMismatch(profileID: String, accountID: String, unixUser: String)
    case missingDefaultProfile(String)
    case managedProfileReadOnly(String)
    case unavailableExecutable(profileID: String, path: String)
    case coreUnavailable(String)

    var userMessage: String {
        switch self {
        case .missingID:
            return "A Terminal Profile id is required."
        case .duplicateID(let id):
            return "Terminal Profile \(id) is duplicated."
        case .missingTitle(let id):
            return "Terminal Profile \(id) needs a title."
        case .missingUnixUser(let id):
            return "Terminal Profile \(id) needs a Unix user."
        case .missingCustomCommand(let id):
            return "Terminal Profile \(id) needs a custom command."
        case .missingManagedAccount(let id):
            return "Terminal Profile \(id) needs a Managed User."
        case .managedAccountMismatch(let id, let accountID, let unixUser):
            return "Terminal Profile \(id) links Managed User \(accountID) but launches \(unixUser)."
        case .missingDefaultProfile(let id):
            return "Default Terminal Profile \(id) is missing."
        case .managedProfileReadOnly(let id):
            return "Managed Terminal Profile \(id) is read-only."
        case .unavailableExecutable(let id, let path):
            return "Terminal Profile \(id) cannot find executable \(path)."
        case .coreUnavailable:
            return "Terminal Profile validation is unavailable."
        }
    }
}

struct TerminalProfileValidationResult: Equatable {
    let errors: [TerminalProfileValidationError]

    var isValid: Bool {
        errors.isEmpty
    }
}

enum TerminalProfileStoreError: Error, Equatable {
    case invalidDocument([TerminalProfileValidationError])
}

enum TerminalProfileStoreRecoveryKind: Equatable {
    case corruptStoreQuarantined
}

struct TerminalProfileStoreRecovery: Equatable {
    let kind: TerminalProfileStoreRecoveryKind
    let evidenceURL: URL
}

struct TerminalProfileLoadResult: Equatable {
    let document: TerminalProfileDocument
    let recovery: TerminalProfileStoreRecovery?

    var profiles: [TerminalProfileDefinition] {
        document.profiles
    }

    func profile(id: String?) -> TerminalProfileDefinition? {
        document.profile(id: id)
    }
}

struct TerminalProfileEditorDraft: Equatable {
    var id: String
    var title: String
    var launchKind: TerminalProfileLaunchKind
    var unixUser: String
    var customCommand: String
    var defaultWorkingDirectory: String?
    var presentation: TerminalProfilePresentation?
    var managedTerminalAccountID: String?

    init(
        id: String = "",
        title: String = "",
        launchKind: TerminalProfileLaunchKind = .loginShell,
        unixUser: String = "",
        customCommand: String = "",
        defaultWorkingDirectory: String? = nil,
        presentation: TerminalProfilePresentation? = nil,
        managedTerminalAccountID: String? = nil
    ) {
        self.id = id
        self.title = title
        self.launchKind = launchKind
        self.unixUser = unixUser
        self.customCommand = customCommand
        self.defaultWorkingDirectory = defaultWorkingDirectory
        self.presentation = presentation
        self.managedTerminalAccountID = managedTerminalAccountID
    }

    init(profile: TerminalProfileDefinition) {
        self.id = profile.id
        self.title = profile.title
        self.launchKind = profile.launch.kind
        self.unixUser = profile.launch.unixUser ?? ""
        self.customCommand = profile.launch.customCommand ?? ""
        self.defaultWorkingDirectory = profile.defaultWorkingDirectory
        self.presentation = profile.presentation
        self.managedTerminalAccountID = profile.managedTerminalAccountID
    }
}

struct TerminalProfileEditorResult: Equatable {
    let definition: TerminalProfileDefinition?
    let errors: [TerminalProfileValidationError]

    var isValid: Bool {
        definition != nil && errors.isEmpty
    }
}

struct TerminalProfileDocumentEditorResult: Equatable {
    let document: TerminalProfileDocument?
    let errors: [TerminalProfileValidationError]

    var isValid: Bool {
        document != nil && errors.isEmpty
    }
}

enum TerminalProfileResolutionState: Equatable {
    case absent
    case resolved
    case missing(requestedID: String)
    case unavailable(requestedID: String, reason: String)

    var environmentValue: String {
        switch self {
        case .absent:
            return "absent"
        case .resolved:
            return "resolved"
        case .missing:
            return "missing"
        case .unavailable:
            return "unavailable"
        }
    }
}
