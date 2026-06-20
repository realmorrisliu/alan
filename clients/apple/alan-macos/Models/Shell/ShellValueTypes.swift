import Foundation

enum TerminalRuntimeControlKey: String, Codable, Equatable {
    case interrupt
    case endOfTransmission = "end_of_transmission"
    case returnKey = "return"
}

enum ShellAttentionState: String, Codable, CaseIterable {
    case idle
    case active
    case awaitingUser = "awaiting_user"
    case notable

    /// Signal-semantics gate (docs/design/design-language.md, "Signal Semantics"):
    /// only states blocked on the user (input/approval) or a failure needing
    /// intervention may surface `ShellSignal.action`. `.active` is quiet
    /// liveness and `.idle` is silence — both must stay inkless in chrome.
    var requiresUserAction: Bool {
        switch self {
        case .awaitingUser, .notable:
            return true
        case .idle, .active:
            return false
        }
    }
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

enum ShellWorkspaceCommand: String, Codable, CaseIterable, Identifiable {
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

    var id: String { rawValue }
}

enum ShellLaunchTarget: String, Codable, CaseIterable {
    case shell
}

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

struct ManagedTerminalAccountRequest: Codable, Equatable {
    let accountName: String
    let guiUserName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool
    let bindCurrentSpaceAfterSuccess: Bool

    init(
        accountName: String,
        guiUserName: String,
        fullName: String? = nil,
        shell: String = "/bin/zsh",
        homeDirectory: String? = nil,
        hideFromLoginWindow: Bool = true,
        bindCurrentSpaceAfterSuccess: Bool = false
    ) {
        self.accountName = accountName
        self.guiUserName = guiUserName
        self.fullName = fullName
        self.shell = shell
        self.homeDirectory = homeDirectory ?? "/Users/\(accountName)"
        self.hideFromLoginWindow = hideFromLoginWindow
        self.bindCurrentSpaceAfterSuccess = bindCurrentSpaceAfterSuccess
    }

    static func canonicalHomeDirectory(for accountName: String) -> String {
        "/Users/\(accountName)"
    }

    var terminalProfileID: String {
        accountName
    }
}

enum ManagedTerminalAccountValidationError: Equatable {
    case invalidAccountName(String)
    case invalidGUIUserName(String)
    case reservedAccountName(String)
    case invalidShell(String)
    case coreUnavailable(String)
}

enum ManagedTerminalAccountIdentifierValidator {
    static func validate(_ request: ManagedTerminalAccountRequest) -> [ManagedTerminalAccountValidationError] {
        do {
            return try ShellCoreFFIAdapter.shared.validateManagedTerminalAccountRequest(request)
        } catch {
            return [.coreUnavailable(String(describing: error))]
        }
    }
}

enum AlanPrivilegedHelperRegistrationAPI: String, Codable, Equatable {
    case smAppServiceDaemon = "SMAppService.daemon(plistName:)"
}

struct AlanPrivilegedHelperIdentity: Codable, Equatable {
    let channelID: String
    let registrationAPI: AlanPrivilegedHelperRegistrationAPI
    let appBundleIdentifier: String
    let helperBundleIdentifier: String
    let launchdServiceLabel: String
    let machServiceName: String
    let plistName: String
    let dataRootPath: String
    let expectedClientRequirement: String
}

extension AlanInstallChannel {
    var privilegedHelperIdentity: AlanPrivilegedHelperIdentity {
        let helperBundleID = "\(bundleIdentifier).privileged-helper"
        return AlanPrivilegedHelperIdentity(
            channelID: installChannelID,
            registrationAPI: .smAppServiceDaemon,
            appBundleIdentifier: bundleIdentifier,
            helperBundleIdentifier: helperBundleID,
            launchdServiceLabel: helperBundleID,
            machServiceName: "\(helperBundleID).xpc",
            plistName: "\(helperBundleID).plist",
            dataRootPath: "/Library/Application Support/\(applicationSupportDirectoryName)/privileged-helper",
            expectedClientRequirement: "identifier \"\(bundleIdentifier)\""
        )
    }
}

enum AlanPrivilegedHelperOperation: String, Codable, Equatable, CaseIterable {
    case helperStatus
    case diagnoseManagedUser
    case applyManagedUserPlan
    case startManagedUserPTY
    case readManagedUserPTY
    case writeManagedUserPTY
    case resizeManagedUserPTY
    case closeManagedUserPTYInput
    case signalManagedUserPTY
    case observeManagedUserPTYExit
    case terminatePTY
    case removeManagedUserIntegration
    case deleteManagedUser
}

enum AlanPrivilegedHelperStatusState: String, Codable, Equatable, CaseIterable {
    case notInstalled = "not_installed"
    case outdated
    case invalidSignature = "invalid_signature"
    case installing
    case updating
    case healthy
    case unavailable
    case uninstallable
}

struct AlanPrivilegedHelperStatus: Codable, Equatable {
    let state: AlanPrivilegedHelperStatusState
    let identity: AlanPrivilegedHelperIdentity
    let installedVersion: String?
    let expectedVersion: String?
    let sanitizedMessage: String?

    var isHealthy: Bool {
        state == .healthy
    }
}

enum AlanPrivilegedHelperErrorCode: String, Codable, Equatable {
    case helperUnavailable = "helper_unavailable"
    case helperOutdated = "helper_outdated"
    case helperSignatureInvalid = "helper_signature_invalid"
    case clientRequirementFailed = "client_requirement_failed"
    case channelMismatch = "channel_mismatch"
    case invalidAccountIdentifier = "invalid_account_identifier"
    case invalidHomePath = "invalid_home_path"
    case shellNotAllowed = "shell_not_allowed"
    case unsupportedOperation = "unsupported_operation"
    case accountNotAlanManaged = "account_not_alan_managed"
    case rawCommandRejected = "raw_command_rejected"
    case rawSudoersRejected = "raw_sudoers_rejected"
    case ptySpawnFailed = "pty_spawn_failed"
    case destructiveConfirmationRequired = "destructive_confirmation_required"
}

struct AlanPrivilegedHelperDiagnostic: Error, Codable, Equatable {
    let operationID: String
    let channelID: String
    let accountName: String?
    let operation: AlanPrivilegedHelperOperation
    let code: AlanPrivilegedHelperErrorCode?
    let sanitizedMessage: String
}

enum AlanManagedUserOwnershipState: String, Codable, Equatable {
    case missing
    case alanManaged = "alan_managed"
    case notAlanManaged = "not_alan_managed"
}

enum AlanManagedUserReadinessState: String, Codable, Equatable {
    case accountMissing = "account_missing"
    case repairable
    case ready
    case accountNotAlanManaged = "account_not_alan_managed"
    case helperUnavailable = "helper_unavailable"
    case legacySudoersPresent = "legacy_sudoers_present"
    case ptySpawnFailed = "pty_spawn_failed"
    case destructiveConfirmationRequired = "destructive_confirmation_required"
}

struct AlanManagedUserDiagnosis: Codable, Equatable {
    let request: ManagedTerminalAccountRequest
    let ownershipState: AlanManagedUserOwnershipState
    let readinessState: AlanManagedUserReadinessState
    let accountExists: Bool
    let homeDirectoryExists: Bool
    let shellMatches: Bool
    let hiddenFromLoginWindow: Bool
    let legacySudoersPath: String?
    let terminalProfileID: String?
    let ptySmokeVerified: Bool
    let diagnostic: AlanPrivilegedHelperDiagnostic?
}

extension AlanManagedUserDiagnosis {
    static func helperUnavailable(
        request: ManagedTerminalAccountRequest,
        status: AlanPrivilegedHelperStatus
    ) -> AlanManagedUserDiagnosis {
        AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .missing,
            readinessState: .helperUnavailable,
            accountExists: false,
            homeDirectoryExists: false,
            shellMatches: false,
            hiddenFromLoginWindow: false,
            legacySudoersPath: nil,
            terminalProfileID: nil,
            ptySmokeVerified: false,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: status.identity.channelID,
                accountName: request.accountName,
                operation: .diagnoseManagedUser,
                code: status.unavailableErrorCode,
                sanitizedMessage: status.sanitizedMessage ?? "Privileged helper is unavailable."
            )
        )
    }
}

private extension AlanPrivilegedHelperStatus {
    var unavailableErrorCode: AlanPrivilegedHelperErrorCode {
        switch state {
        case .outdated:
            return .helperOutdated
        case .invalidSignature:
            return .helperSignatureInvalid
        case .notInstalled, .installing, .updating, .healthy, .unavailable, .uninstallable:
            return .helperUnavailable
        }
    }
}

enum AlanManagedUserHelperPlanStepKind: String, Codable, Equatable, CaseIterable {
    case createStandardAccount = "create_standard_account"
    case repairAccountType = "repair_account_type"
    case repairHomeDirectory = "repair_home_directory"
    case repairShell = "repair_shell"
    case hideAccount = "hide_account"
    case writeOwnershipMarker = "write_ownership_marker"
    case verifyAccount = "verify_account"
    case cleanupLegacySudoers = "cleanup_legacy_sudoers"
    case verifyManagedUserPTY = "verify_managed_user_pty"
    case removeManagedUserIntegration = "remove_managed_user_integration"
    case deleteAccount = "delete_account"
    case deleteHomeDirectory = "delete_home_directory"
}

struct AlanManagedUserHelperPlanStep: Codable, Equatable {
    let kind: AlanManagedUserHelperPlanStepKind
    let summary: String
    let requiresDestructiveConfirmation: Bool
}

struct AlanManagedUserHelperPlan: Codable, Equatable {
    let operationID: String
    let channelID: String
    let request: ManagedTerminalAccountRequest
    let steps: [AlanManagedUserHelperPlanStep]
}

struct AlanManagedUserPTYStartRequest: Codable, Equatable {
    let operationID: String
    let channelID: String
    let accountName: String
    let homeDirectory: String
    let shell: String
    let contentID: String
    let columns: Int
    let rows: Int
}

struct AlanManagedUserPTYSession: Codable, Equatable {
    let sessionID: String
    let accountName: String
    let contentID: String
    let helperOwnsChildProcess: Bool
    let sanitizedMessage: String
}

struct AlanManagedUserPTYInputRequest: Codable, Equatable {
    let sessionID: String
    let text: String
}

struct AlanManagedUserPTYReadRequest: Codable, Equatable {
    let sessionID: String
    let maxBytes: Int
}

struct AlanManagedUserPTYOutputChunk: Codable, Equatable {
    let sessionID: String
    let data: Data
    let final: Bool
    let sanitizedMessage: String?
}

struct AlanManagedUserPTYResizeRequest: Codable, Equatable {
    let sessionID: String
    let columns: Int
    let rows: Int
}

enum AlanManagedUserPTYSignal: String, Codable, Equatable {
    case interrupt
    case terminate
    case kill
}

struct AlanManagedUserPTYSignalRequest: Codable, Equatable {
    let sessionID: String
    let signal: AlanManagedUserPTYSignal
}

struct AlanManagedUserPTYExitObservation: Codable, Equatable {
    let sessionID: String
    let final: Bool
    let exitCode: Int32?
    let terminatingSignal: Int32?
    let sanitizedMessage: String?
}

struct AlanManagedUserPTYControlResult: Codable, Equatable {
    let accepted: Bool
    let diagnostic: AlanPrivilegedHelperDiagnostic

    static func accepted(
        operation: AlanPrivilegedHelperOperation,
        channelID: String,
        accountName: String?,
        message: String
    ) -> AlanManagedUserPTYControlResult {
        AlanManagedUserPTYControlResult(
            accepted: true,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: channelID,
                accountName: accountName,
                operation: operation,
                code: nil,
                sanitizedMessage: message
            )
        )
    }

    static func rejected(
        operation: AlanPrivilegedHelperOperation,
        channelID: String,
        accountName: String?,
        code: AlanPrivilegedHelperErrorCode,
        message: String
    ) -> AlanManagedUserPTYControlResult {
        AlanManagedUserPTYControlResult(
            accepted: false,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: channelID,
                accountName: accountName,
                operation: operation,
                code: code,
                sanitizedMessage: message
            )
        )
    }
}

enum AlanPrivilegedHelperRequestValidator {
    static let allowedShells: Set<String> = ["/bin/zsh"]

    static func validate(
        request: ManagedTerminalAccountRequest,
        channel: AlanInstallChannel
    ) -> [AlanPrivilegedHelperErrorCode] {
        var errors: [AlanPrivilegedHelperErrorCode] = []
        if !ManagedTerminalAccountIdentifierValidator.validate(request).isEmpty {
            errors.append(.invalidAccountIdentifier)
        }
        if request.homeDirectory != ManagedTerminalAccountRequest.canonicalHomeDirectory(
            for: request.accountName
        ) {
            errors.append(.invalidHomePath)
        }
        if !allowedShells.contains(request.shell) {
            errors.append(.shellNotAllowed)
        }
        if channel.privilegedHelperIdentity.channelID != channel.installChannelID {
            errors.append(.channelMismatch)
        }
        return errors
    }

    static func rejectsRawPrivilegedPayload(_ payload: String) -> AlanPrivilegedHelperErrorCode? {
        let lowered = payload.lowercased()
        if lowered.contains("do shell script") || lowered.contains("#!/bin/sh") || lowered.contains("sudo ") {
            return .rawCommandRejected
        }
        if lowered.contains("/etc/sudoers") || lowered.contains("nopasswd") {
            return .rawSudoersRejected
        }
        return nil
    }
}

protocol AlanPrivilegedHelperClienting {
    func status() -> AlanPrivilegedHelperStatus
    func diagnoseManagedUser(_ request: ManagedTerminalAccountRequest) -> AlanManagedUserDiagnosis
    func applyManagedUserPlan(_ plan: AlanManagedUserHelperPlan) -> ManagedTerminalAccountApplyResult
    func startManagedUserPTY(_ request: AlanManagedUserPTYStartRequest) -> Result<AlanManagedUserPTYSession, AlanPrivilegedHelperDiagnostic>
    func readManagedUserPTY(_ request: AlanManagedUserPTYReadRequest) -> Result<AlanManagedUserPTYOutputChunk, AlanPrivilegedHelperDiagnostic>
    func writeManagedUserPTY(_ request: AlanManagedUserPTYInputRequest) -> AlanManagedUserPTYControlResult
    func resizeManagedUserPTY(_ request: AlanManagedUserPTYResizeRequest) -> AlanManagedUserPTYControlResult
    func closeManagedUserPTYInput(sessionID: String) -> AlanManagedUserPTYControlResult
    func signalManagedUserPTY(_ request: AlanManagedUserPTYSignalRequest) -> AlanManagedUserPTYControlResult
    func observeManagedUserPTYExit(sessionID: String) -> AlanManagedUserPTYExitObservation?
    func terminatePTY(sessionID: String) -> AlanPrivilegedHelperDiagnostic
    func removeManagedUserIntegration(_ request: ManagedTerminalAccountRequest) -> ManagedTerminalAccountApplyResult
}

final class AlanPrivilegedHelperFakeClient: AlanPrivilegedHelperClienting {
    var helperStatus: AlanPrivilegedHelperStatus
    var diagnosesByAccount: [String: AlanManagedUserDiagnosis]
    var deniedOperation: AlanPrivilegedHelperOperation?
    var appliedPlans: [AlanManagedUserHelperPlan] = []
    var startedPTYRequests: [AlanManagedUserPTYStartRequest] = []
    var readPTYRequests: [AlanManagedUserPTYReadRequest] = []
    var writtenPTYInputRequests: [AlanManagedUserPTYInputRequest] = []
    var resizedPTYRequests: [AlanManagedUserPTYResizeRequest] = []
    var closedPTYInputSessionIDs: [String] = []
    var signaledPTYRequests: [AlanManagedUserPTYSignalRequest] = []
    var terminatedPTYSessionIDs: [String] = []
    var exitObservationsBySessionID: [String: AlanManagedUserPTYExitObservation] = [:]
    var outputChunksBySessionID: [String: [Data]] = [:]
    private var startedPTYSessionAccounts: [String: String] = [:]

    init(
        channel: AlanInstallChannel = .current(),
        statusState: AlanPrivilegedHelperStatusState = .healthy,
        diagnosesByAccount: [String: AlanManagedUserDiagnosis] = [:]
    ) {
        helperStatus = AlanPrivilegedHelperStatus(
            state: statusState,
            identity: channel.privilegedHelperIdentity,
            installedVersion: nil,
            expectedVersion: nil,
            sanitizedMessage: nil
        )
        self.diagnosesByAccount = diagnosesByAccount
    }

    func status() -> AlanPrivilegedHelperStatus {
        helperStatus
    }

    func diagnoseManagedUser(_ request: ManagedTerminalAccountRequest) -> AlanManagedUserDiagnosis {
        if let diagnosis = diagnosesByAccount[request.accountName] {
            return diagnosis
        }
        return AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .missing,
            readinessState: helperStatus.isHealthy ? .accountMissing : .helperUnavailable,
            accountExists: false,
            homeDirectoryExists: false,
            shellMatches: false,
            hiddenFromLoginWindow: false,
            legacySudoersPath: nil,
            terminalProfileID: nil,
            ptySmokeVerified: false,
            diagnostic: helperStatus.isHealthy ? nil : diagnostic(
                operation: .diagnoseManagedUser,
                accountName: request.accountName,
                code: .helperUnavailable,
                message: "Privileged helper is unavailable."
            )
        )
    }

    func applyManagedUserPlan(_ plan: AlanManagedUserHelperPlan) -> ManagedTerminalAccountApplyResult {
        if deniedOperation == .applyManagedUserPlan || !helperStatus.isHealthy {
            return ManagedTerminalAccountApplyResult(
                completedSteps: [],
                failedStep: plan.steps.first.map { .helperStep($0.kind) },
                cancelled: false,
                visibleDiagnostics: ["Privileged helper rejected the Managed User plan. Credentials redacted."]
            )
        }
        appliedPlans.append(plan)
        return ManagedTerminalAccountApplyResult(
            completedSteps: plan.steps.map { ManagedTerminalAccountPlanStepKind.helperStep($0.kind) },
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper applied the Managed User plan. Credentials redacted."]
        )
    }

    func startManagedUserPTY(
        _ request: AlanManagedUserPTYStartRequest
    ) -> Result<AlanManagedUserPTYSession, AlanPrivilegedHelperDiagnostic> {
        guard helperStatus.isHealthy, deniedOperation != .startManagedUserPTY else {
            return .failure(
                diagnostic(
                    operation: .startManagedUserPTY,
                    accountName: request.accountName,
                    code: .ptySpawnFailed,
                    message: "Privileged helper could not start the managed-user PTY."
                )
            )
        }
        startedPTYRequests.append(request)
        let session = AlanManagedUserPTYSession(
            sessionID: "fake-\(request.contentID)",
            accountName: request.accountName,
            contentID: request.contentID,
            helperOwnsChildProcess: true,
            sanitizedMessage: "Fake helper PTY session started."
        )
        startedPTYSessionAccounts[session.sessionID] = session.accountName
        return .success(session)
    }

    func readManagedUserPTY(
        _ request: AlanManagedUserPTYReadRequest
    ) -> Result<AlanManagedUserPTYOutputChunk, AlanPrivilegedHelperDiagnostic> {
        readPTYRequests.append(request)
        guard helperStatus.isHealthy, deniedOperation != .readManagedUserPTY else {
            return .failure(
                diagnostic(
                    operation: .readManagedUserPTY,
                    accountName: startedPTYSessionAccounts[request.sessionID],
                    code: helperStatus.isHealthy ? .unsupportedOperation : .helperUnavailable,
                    message: "Privileged helper rejected the managed-user PTY read request."
                )
            )
        }
        var queued = outputChunksBySessionID[request.sessionID] ?? []
        let data = queued.isEmpty ? Data() : queued.removeFirst()
        outputChunksBySessionID[request.sessionID] = queued
        return .success(
            AlanManagedUserPTYOutputChunk(
                sessionID: request.sessionID,
                data: data,
                final: exitObservationsBySessionID[request.sessionID]?.final == true,
                sanitizedMessage: data.isEmpty ? nil : "Privileged helper returned PTY output."
            )
        )
    }

    func writeManagedUserPTY(_ request: AlanManagedUserPTYInputRequest) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .writeManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper accepted PTY input."
        )
        if result.accepted {
            writtenPTYInputRequests.append(request)
        }
        return result
    }

    func resizeManagedUserPTY(_ request: AlanManagedUserPTYResizeRequest) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .resizeManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper resized PTY session."
        )
        if result.accepted {
            resizedPTYRequests.append(request)
        }
        return result
    }

    func closeManagedUserPTYInput(sessionID: String) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .closeManagedUserPTYInput,
            sessionID: sessionID,
            successMessage: "Privileged helper closed PTY input."
        )
        if result.accepted {
            closedPTYInputSessionIDs.append(sessionID)
        }
        return result
    }

    func signalManagedUserPTY(
        _ request: AlanManagedUserPTYSignalRequest
    ) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .signalManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper signaled PTY session."
        )
        if result.accepted {
            signaledPTYRequests.append(request)
        }
        return result
    }

    func observeManagedUserPTYExit(sessionID: String) -> AlanManagedUserPTYExitObservation? {
        exitObservationsBySessionID[sessionID]
    }

    func terminatePTY(sessionID: String) -> AlanPrivilegedHelperDiagnostic {
        terminatedPTYSessionIDs.append(sessionID)
        exitObservationsBySessionID[sessionID] = AlanManagedUserPTYExitObservation(
            sessionID: sessionID,
            final: true,
            exitCode: nil,
            terminatingSignal: nil,
            sanitizedMessage: "Privileged helper terminated PTY session."
        )
        return diagnostic(
            operation: .terminatePTY,
            accountName: startedPTYSessionAccounts[sessionID],
            code: nil,
            message: "Privileged helper terminated PTY session \(sessionID)."
        )
    }

    func removeManagedUserIntegration(
        _ request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountApplyResult {
        ManagedTerminalAccountApplyResult(
            completedSteps: [.removeManagedTerminalProfile],
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper removed Managed User integration. Credentials redacted."]
        )
    }

    private func controlResult(
        operation: AlanPrivilegedHelperOperation,
        sessionID: String,
        successMessage: String
    ) -> AlanManagedUserPTYControlResult {
        let accountName = startedPTYSessionAccounts[sessionID]
        guard helperStatus.isHealthy, deniedOperation != operation else {
            return .rejected(
                operation: operation,
                channelID: helperStatus.identity.channelID,
                accountName: accountName,
                code: helperStatus.isHealthy ? .unsupportedOperation : .helperUnavailable,
                message: "Privileged helper rejected the managed-user PTY request."
            )
        }
        return .accepted(
            operation: operation,
            channelID: helperStatus.identity.channelID,
            accountName: accountName,
            message: successMessage
        )
    }

    private func diagnostic(
        operation: AlanPrivilegedHelperOperation,
        accountName: String?,
        code: AlanPrivilegedHelperErrorCode?,
        message: String
    ) -> AlanPrivilegedHelperDiagnostic {
        AlanPrivilegedHelperDiagnostic(
            operationID: UUID().uuidString,
            channelID: helperStatus.identity.channelID,
            accountName: accountName,
            operation: operation,
            code: code,
            sanitizedMessage: message
        )
    }
}

struct ManagedTerminalAccountCommandResult: Equatable {
    let exitCode: Int32
    let standardOutput: String
    let standardError: String

    var succeeded: Bool {
        exitCode == 0
    }

    var combinedOutput: String {
        [standardOutput, standardError]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
    }
}

protocol ManagedTerminalAccountCommandRunning {
    func run(
        executablePath: String,
        arguments: [String]
    ) -> ManagedTerminalAccountCommandResult
}

struct ManagedTerminalAccountProcessRunner: ManagedTerminalAccountCommandRunning {
    let timeoutSeconds: TimeInterval

    init(timeoutSeconds: TimeInterval = 120) {
        self.timeoutSeconds = timeoutSeconds
    }

    func run(
        executablePath: String,
        arguments: [String]
    ) -> ManagedTerminalAccountCommandResult {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executablePath)
        process.arguments = arguments

        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe

        do {
            try process.run()
            let deadline = Date().addingTimeInterval(timeoutSeconds)
            while process.isRunning {
                if Date() >= deadline {
                    process.terminate()
                    return ManagedTerminalAccountCommandResult(
                        exitCode: 124,
                        standardOutput: "",
                        standardError: "Command timed out after \(timeoutSeconds) seconds."
                    )
                }
                Thread.sleep(forTimeInterval: 0.05)
            }
        } catch {
            return ManagedTerminalAccountCommandResult(
                exitCode: 127,
                standardOutput: "",
                standardError: "\(error)"
            )
        }

        let output = String(
            data: outputPipe.fileHandleForReading.readDataToEndOfFile(),
            encoding: .utf8
        ) ?? ""
        let error = String(
            data: errorPipe.fileHandleForReading.readDataToEndOfFile(),
            encoding: .utf8
        ) ?? ""
        return ManagedTerminalAccountCommandResult(
            exitCode: process.terminationStatus,
            standardOutput: output,
            standardError: error
        )
    }
}

enum ManagedTerminalAccountRecord: Equatable {
    case missing
    case standard(homeDirectory: String, shell: String, hidden: Bool)
    case admin(homeDirectory: String, shell: String, hidden: Bool)
    case invalid(reason: String)
}

enum ManagedTerminalAccountSudoersState: Equatable {
    case missing
    case alanOwnedValid(path: String)
    case alanOwnedInvalid(path: String, message: String)
    case unmanaged(path: String)
    case existingUnreadable(path: String)
}

enum ManagedTerminalAccountOwnershipEvidence: Equatable {
    case helperMarker(path: String)
    case legacyAlanSudoers(path: String)
}

enum ManagedTerminalAccountOwnershipState: Equatable {
    case missing
    case alanManaged(ManagedTerminalAccountOwnershipEvidence)
    case notAlanManaged(reason: String)

    var isAlanManaged: Bool {
        if case .alanManaged = self {
            return true
        }
        return false
    }
}

enum ManagedTerminalAccountProfileState: Equatable {
    case missing
    case existingManaged(profileID: String)
    case existingManagedOutdated(profileID: String)
    case existingUnmanaged(profileID: String)
}

enum ManagedTerminalAccountVerificationStep: String, Equatable {
    case accountLookup = "account_lookup"
    case nonAdminAccount = "non_admin_account"
    case homeDirectory = "home_directory"
    case shell
    case ownership
    case sudoersValidation = "sudoers_validation"
    case nonInteractiveSudo = "non_interactive_sudo"
}

enum ManagedTerminalAccountVerificationStatus: Equatable {
    case notRun
    case passed
    case failed(step: ManagedTerminalAccountVerificationStep, message: String)
}

struct ManagedTerminalAccountState: Equatable {
    let account: ManagedTerminalAccountRecord
    let sudoers: ManagedTerminalAccountSudoersState
    let ownership: ManagedTerminalAccountOwnershipState
    let terminalProfile: ManagedTerminalAccountProfileState
    let verification: ManagedTerminalAccountVerificationStatus
    let homeDirectoryExists: Bool

    init(
        account: ManagedTerminalAccountRecord,
        sudoers: ManagedTerminalAccountSudoersState,
        ownership: ManagedTerminalAccountOwnershipState = .missing,
        terminalProfile: ManagedTerminalAccountProfileState,
        verification: ManagedTerminalAccountVerificationStatus,
        homeDirectoryExists: Bool = true
    ) {
        self.account = account
        self.sudoers = sudoers
        self.ownership = ownership
        self.terminalProfile = terminalProfile
        self.verification = verification
        self.homeDirectoryExists = homeDirectoryExists
    }
}

struct ManagedTerminalAccountSudoersValidationResult: Equatable {
    let isValid: Bool
    let message: String?

    static let passed = ManagedTerminalAccountSudoersValidationResult(isValid: true, message: nil)

    static func failed(_ message: String) -> ManagedTerminalAccountSudoersValidationResult {
        ManagedTerminalAccountSudoersValidationResult(isValid: false, message: message)
    }
}

protocol ManagedTerminalAccountSudoersSyntaxChecking {
    func validateSudoersFile(atPath path: String) -> ManagedTerminalAccountSudoersValidationResult
}

struct ManagedTerminalAccountVisudoSyntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking {
    let commandRunner: ManagedTerminalAccountCommandRunning

    init(commandRunner: ManagedTerminalAccountCommandRunning = ManagedTerminalAccountProcessRunner()) {
        self.commandRunner = commandRunner
    }

    func validateSudoersFile(atPath path: String) -> ManagedTerminalAccountSudoersValidationResult {
        let result = commandRunner.run(
            executablePath: "/usr/sbin/visudo",
            arguments: ["-cf", path]
        )
        guard result.succeeded else {
            let message = result.combinedOutput.isEmpty ? "visudo validation failed." : result.combinedOutput
            return .failed(message)
        }
        return .passed
    }
}

enum ManagedTerminalAccountSudoersValidator {
    static func state(
        request: ManagedTerminalAccountRequest,
        fileManager: FileManager,
        syntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking
    ) -> ManagedTerminalAccountSudoersState {
        let rule = ManagedTerminalAccountSudoersRule(request: request)
        guard fileManager.fileExists(atPath: rule.filePath) else {
            return .missing
        }

        guard let data = fileManager.contents(atPath: rule.filePath),
              let contents = String(data: data, encoding: .utf8)
        else {
            return .existingUnreadable(path: rule.filePath)
        }

        guard contents.contains(ManagedTerminalAccountSudoersRule.managedMarker) else {
            return .unmanaged(path: rule.filePath)
        }

        let validation = validate(contents: contents, rule: rule, syntaxChecker: syntaxChecker)
        if validation.isValid {
            return .alanOwnedValid(path: rule.filePath)
        }
        return .alanOwnedInvalid(
            path: rule.filePath,
            message: validation.message ?? "Sudoers validation failed."
        )
    }

    static func validate(
        contents: String,
        rule: ManagedTerminalAccountSudoersRule,
        syntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking
    ) -> ManagedTerminalAccountSudoersValidationResult {
        let normalizedExpected = normalizedSudoersContents(rule.contents)
        let normalizedActual = normalizedSudoersContents(contents)
        guard normalizedActual == normalizedExpected else {
            return .failed("Alan-owned sudoers drop-in does not match the requested terminal account.")
        }
        return syntaxChecker.validateSudoersFile(atPath: rule.filePath)
    }

    private static func normalizedSudoersContents(_ contents: String) -> String {
        contents.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

struct ManagedTerminalAccountLocalStateDiscoverer {
    let fileManager: FileManager
    let commandRunner: ManagedTerminalAccountCommandRunning
    let sudoersSyntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking
    let helperIdentity: AlanPrivilegedHelperIdentity

    init(
        fileManager: FileManager = .default,
        commandRunner: ManagedTerminalAccountCommandRunning = ManagedTerminalAccountProcessRunner(),
        sudoersSyntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking = ManagedTerminalAccountVisudoSyntaxChecker(),
        channel: AlanInstallChannel = .current()
    ) {
        self.fileManager = fileManager
        self.commandRunner = commandRunner
        self.sudoersSyntaxChecker = sudoersSyntaxChecker
        self.helperIdentity = channel.privilegedHelperIdentity
    }

    func discover(
        request: ManagedTerminalAccountRequest,
        terminalProfiles: TerminalProfileDocument
    ) -> ManagedTerminalAccountState {
        let validationErrors = ManagedTerminalAccountIdentifierValidator.validate(request)
        guard validationErrors.isEmpty else {
            return ManagedTerminalAccountState(
                account: .invalid(reason: "Invalid managed terminal account request."),
                sudoers: .missing,
                terminalProfile: .missing,
                verification: .notRun
            )
        }

        let account = accountRecord(for: request)
        let sudoers = ManagedTerminalAccountSudoersValidator.state(
            request: request,
            fileManager: fileManager,
            syntaxChecker: sudoersSyntaxChecker
        )
        return ManagedTerminalAccountState(
            account: account,
            sudoers: sudoers,
            ownership: ownershipState(for: request, account: account, sudoers: sudoers),
            terminalProfile: terminalProfileState(for: request, document: terminalProfiles),
            verification: .notRun,
            homeDirectoryExists: homeDirectoryExists(for: account)
        )
    }

    private func homeDirectoryExists(for account: ManagedTerminalAccountRecord) -> Bool {
        switch account {
        case .standard(let homeDirectory, _, _), .admin(let homeDirectory, _, _):
            return fileManager.fileExists(atPath: homeDirectory)
        case .missing, .invalid:
            return false
        }
    }

    private func ownershipState(
        for request: ManagedTerminalAccountRequest,
        account: ManagedTerminalAccountRecord,
        sudoers: ManagedTerminalAccountSudoersState
    ) -> ManagedTerminalAccountOwnershipState {
        guard account.requiresAlanManagedOwnership else {
            return .missing
        }

        let markerPath = ownershipMarkerPath(for: request)
        if fileManager.fileExists(atPath: markerPath) {
            return .alanManaged(.helperMarker(path: markerPath))
        }

        if case .alanOwnedValid(let path) = sudoers {
            return .alanManaged(.legacyAlanSudoers(path: path))
        }

        return .notAlanManaged(
            reason: "\(request.accountName) is an existing local account without Alan-managed ownership evidence."
        )
    }

    private func ownershipMarkerPath(for request: ManagedTerminalAccountRequest) -> String {
        "\(helperIdentity.dataRootPath)/managed-users/\(request.accountName)/ownership.json"
    }

    private func accountRecord(for request: ManagedTerminalAccountRequest) -> ManagedTerminalAccountRecord {
        let dscl = commandRunner.run(
            executablePath: "/usr/bin/dscl",
            arguments: [
                ".",
                "-read",
                "/Users/\(request.accountName)",
                "UniqueID",
                "PrimaryGroupID",
                "NFSHomeDirectory",
                "UserShell",
                "IsHidden",
                "AuthenticationAuthority",
            ]
        )
        guard dscl.succeeded else {
            return .missing
        }

        let output = dscl.standardOutput
        guard propertyValue("UniqueID", in: output) != nil,
              propertyValue("PrimaryGroupID", in: output) != nil
        else {
            return .invalid(reason: "Local account record is incomplete.")
        }
        let home = propertyValue("NFSHomeDirectory", in: output) ?? request.homeDirectory
        let shell = propertyValue("UserShell", in: output) ?? request.shell
        let hidden = propertyValue("IsHidden", in: output) == "1"
        let isAdmin = adminMembership(for: request.accountName)
        if isAdmin {
            return .admin(homeDirectory: home, shell: shell, hidden: hidden)
        }
        return .standard(homeDirectory: home, shell: shell, hidden: hidden)
    }

    private func adminMembership(for accountName: String) -> Bool {
        let result = commandRunner.run(
            executablePath: "/usr/sbin/dseditgroup",
            arguments: ["-o", "checkmember", "-m", accountName, "admin"]
        )
        guard result.succeeded else { return false }
        let output = result.combinedOutput.lowercased()
        return output.contains("yes") || (output.contains("is a member") && !output.contains("not a member"))
    }

    private func propertyValue(_ key: String, in output: String) -> String? {
        output
            .split(separator: "\n", omittingEmptySubsequences: false)
            .compactMap { line -> String? in
                let prefixes = ["\(key):", "dsAttrTypeNative:\(key):"]
                guard let prefix = prefixes.first(where: { line.hasPrefix($0) }) else {
                    return nil
                }
                let value = line.dropFirst(prefix.count)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return value.isEmpty ? nil : value
            }
            .first
    }

    private func terminalProfileState(
        for request: ManagedTerminalAccountRequest,
        document: TerminalProfileDocument
    ) -> ManagedTerminalAccountProfileState {
        guard let profile = document.profile(id: request.terminalProfileID) else {
            return .missing
        }
        if profile.managedTerminalAccountID == request.accountName {
            if profile.launch == .sudoUser(unixUser: request.accountName) {
                return .existingManagedOutdated(profileID: profile.id)
            }
            guard profile.launch == .managedUser(unixUser: request.accountName) else {
                return .existingManagedOutdated(profileID: profile.id)
            }
            if profile.defaultWorkingDirectory != request.homeDirectory {
                return .existingManagedOutdated(profileID: profile.id)
            }
            return .existingManaged(profileID: profile.id)
        }
        return .existingUnmanaged(profileID: profile.id)
    }
}

private extension ManagedTerminalAccountRecord {
    var requiresAlanManagedOwnership: Bool {
        switch self {
        case .standard, .admin:
            return true
        case .missing, .invalid:
            return false
        }
    }
}

protocol ManagedTerminalAccountEntryVerifying {
    func verifyTerminalEntry(
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountSudoersValidationResult
}

struct ManagedTerminalAccountSudoEntryVerifier: ManagedTerminalAccountEntryVerifying {
    let commandRunner: ManagedTerminalAccountCommandRunning

    init(commandRunner: ManagedTerminalAccountCommandRunning = ManagedTerminalAccountProcessRunner()) {
        self.commandRunner = commandRunner
    }

    func verifyTerminalEntry(
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountSudoersValidationResult {
        let result = commandRunner.run(
            executablePath: "/usr/bin/sudo",
            arguments: ["-n", "-iu", request.accountName, "true"]
        )
        guard result.succeeded else {
            let message = result.combinedOutput.isEmpty
                ? "Non-interactive sudo verification failed."
                : result.combinedOutput
            return .failed(message)
        }
        return .passed
    }
}

enum ManagedTerminalAccountReadinessVerifier {
    static func verify(
        request: ManagedTerminalAccountRequest,
        state: ManagedTerminalAccountState,
        entryVerifier: ManagedTerminalAccountEntryVerifying
    ) -> ManagedTerminalAccountVerificationStatus {
        switch state.account {
        case .missing:
            return .failed(step: .accountLookup, message: "Managed terminal account is missing.")
        case .admin:
            return .failed(step: .nonAdminAccount, message: "Managed terminal account must be standard.")
        case .invalid(let reason):
            return .failed(step: .accountLookup, message: reason)
        case .standard(let homeDirectory, let shell, _):
            if homeDirectory != request.homeDirectory {
                return .failed(step: .homeDirectory, message: "Home directory does not match the plan.")
            }
            if !state.homeDirectoryExists {
                return .failed(step: .homeDirectory, message: "Home directory is missing.")
            }
            if shell != request.shell {
                return .failed(step: .shell, message: "Login shell does not match the plan.")
            }
        }

        if state.account.requiresAlanManagedOwnership && !state.ownership.isAlanManaged {
            return .failed(
                step: .ownership,
                message: "Local account is not Alan managed."
            )
        }

        switch state.sudoers {
        case .alanOwnedValid, .existingUnreadable:
            break
        case .alanOwnedInvalid(_, let message):
            return .failed(step: .sudoersValidation, message: message)
        case .missing:
            return .failed(step: .sudoersValidation, message: "Alan-owned sudoers drop-in is missing.")
        case .unmanaged:
            return .failed(step: .sudoersValidation, message: "Sudoers drop-in is not Alan-owned.")
        }

        let entry = entryVerifier.verifyTerminalEntry(request: request)
        guard entry.isValid else {
            return .failed(
                step: .nonInteractiveSudo,
                message: entry.message ?? "Non-interactive sudo verification failed."
            )
        }
        return .passed
    }
}

enum ManagedTerminalAccountPlanStepKind: Equatable {
    case createStandardAccount
    case repairAccountType
    case repairHomeDirectory
    case repairShell
    case hideAccount
    case writeSudoersDropIn
    case validateSudoers
    case verifyTerminalEntry
    case createOrUpdateTerminalProfile
    case bindCurrentSpace
    case removeSudoersDropIn
    case removeManagedTerminalProfile
    case deleteAccount
    case deleteHomeDirectory
    case helperStep(AlanManagedUserHelperPlanStepKind)
}

struct ManagedTerminalAccountPlanStep: Equatable {
    let kind: ManagedTerminalAccountPlanStepKind
    let summary: String
    let requiresPrivilege: Bool
}

enum ManagedTerminalAccountPlanStatus: Equatable {
    case readyToApply
    case alreadyReady
    case repair
    case invalid([ManagedTerminalAccountValidationError])
    case helperUnavailable
    case accountNotAlanManaged
    case legacySudoersPresent(path: String?)
    case ptySpawnFailed
    case requiresDestructiveConfirmation
    case sudoersConflict(path: String)
    case terminalProfileConflict(profileID: String)
}

struct ManagedTerminalAccountPlan: Equatable {
    let request: ManagedTerminalAccountRequest
    let status: ManagedTerminalAccountPlanStatus
    let steps: [ManagedTerminalAccountPlanStep]
}

enum ManagedTerminalAccountRollbackScope: Equatable {
    case alanIntegrationOnly
    case deleteAccountAndHome(confirmation: String?)
}

enum ManagedTerminalAccountPlanner {
    static func plan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis
    ) -> ManagedTerminalAccountPlan {
        let validationErrors = ManagedTerminalAccountIdentifierValidator.validate(request)
        guard validationErrors.isEmpty else {
            return ManagedTerminalAccountPlan(request: request, status: .invalid(validationErrors), steps: [])
        }

        switch diagnosis.readinessState {
        case .helperUnavailable:
            return ManagedTerminalAccountPlan(request: request, status: .helperUnavailable, steps: [])
        case .accountNotAlanManaged:
            return ManagedTerminalAccountPlan(request: request, status: .accountNotAlanManaged, steps: [])
        case .destructiveConfirmationRequired:
            return ManagedTerminalAccountPlan(
                request: request,
                status: .requiresDestructiveConfirmation,
                steps: helperBackedSteps(request: request, diagnosis: diagnosis)
            )
        case .ready:
            let steps = terminalProfileHandoffSteps(request: request, diagnosis: diagnosis)
            return ManagedTerminalAccountPlan(
                request: request,
                status: steps.isEmpty ? .alreadyReady : .readyToApply,
                steps: steps
            )
        case .legacySudoersPresent:
            if let conflictPath = unexpectedLegacySudoersPath(request: request, diagnosis: diagnosis) {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .sudoersConflict(path: conflictPath),
                    steps: []
                )
            }
            return ManagedTerminalAccountPlan(
                request: request,
                status: .legacySudoersPresent(path: diagnosis.legacySudoersPath),
                steps: helperBackedSteps(request: request, diagnosis: diagnosis)
            )
        case .ptySpawnFailed:
            return ManagedTerminalAccountPlan(
                request: request,
                status: .ptySpawnFailed,
                steps: helperBackedSteps(request: request, diagnosis: diagnosis)
            )
        case .accountMissing, .repairable:
            let steps = helperBackedSteps(request: request, diagnosis: diagnosis)
            return ManagedTerminalAccountPlan(
                request: request,
                status: diagnosis.accountExists ? .repair : .readyToApply,
                steps: steps
            )
        }
    }

    static func plan(
        request: ManagedTerminalAccountRequest,
        state: ManagedTerminalAccountState
    ) -> ManagedTerminalAccountPlan {
        do {
            return try ShellCoreFFIAdapter.shared.managedTerminalAccountPlan(
                request: request,
                state: state
            )
        } catch {
            return ManagedTerminalAccountPlan(
                request: request,
                status: .invalid([.coreUnavailable(String(describing: error))]),
                steps: []
            )
        }
    }

    static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope
    ) -> ManagedTerminalAccountPlan {
        if diagnosis.readinessState == .helperUnavailable {
            return ManagedTerminalAccountPlan(request: request, status: .helperUnavailable, steps: [])
        }
        if diagnosis.readinessState == .accountNotAlanManaged
            || diagnosis.ownershipState == .notAlanManaged
        {
            return ManagedTerminalAccountPlan(request: request, status: .accountNotAlanManaged, steps: [])
        }

        var steps: [ManagedTerminalAccountPlanStep] = []
        if shouldCleanupLegacySudoers(request: request, diagnosis: diagnosis) {
            steps.append(helperStep(.cleanupLegacySudoers, "Clean up verified legacy Alan sudoers"))
        }
        if diagnosis.terminalProfileID == request.terminalProfileID {
            steps.append(step(.removeManagedTerminalProfile, "Remove managed Terminal Profile", false))
        }
        steps.append(helperStep(.removeManagedUserIntegration, "Remove helper-managed account integration"))

        switch scope {
        case .alanIntegrationOnly:
            return ManagedTerminalAccountPlan(request: request, status: .readyToApply, steps: steps)
        case .deleteAccountAndHome(let confirmation):
            guard diagnosis.ownershipState == .alanManaged else {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .accountNotAlanManaged,
                    steps: steps
                )
            }
            guard confirmation == request.accountName else {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .requiresDestructiveConfirmation,
                    steps: steps
                )
            }
            var destructiveSteps: [ManagedTerminalAccountPlanStep] = []
            if diagnosis.accountExists {
                destructiveSteps.append(helperStep(.deleteAccount, "Delete terminal account"))
            }
            if diagnosis.homeDirectoryExists
                && request.homeDirectory == ManagedTerminalAccountRequest.canonicalHomeDirectory(
                    for: request.accountName
                )
            {
                destructiveSteps.append(
                    helperStep(.deleteHomeDirectory, "Delete terminal account home directory")
                )
            }
            return ManagedTerminalAccountPlan(
                request: request,
                status: .readyToApply,
                steps: steps + destructiveSteps
            )
        }
    }

    static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        state: ManagedTerminalAccountState,
        scope: ManagedTerminalAccountRollbackScope
    ) -> ManagedTerminalAccountPlan {
        var steps: [ManagedTerminalAccountPlanStep] = []
        if shouldRemoveSudoersDropIn(request: request, sudoers: state.sudoers) {
            steps.append(step(.removeSudoersDropIn, "Remove Alan-owned sudoers drop-in", true))
        }
        if case .existingManaged = state.terminalProfile {
            steps.append(step(.removeManagedTerminalProfile, "Remove managed Terminal Profile", false))
        } else if case .existingManagedOutdated = state.terminalProfile {
            steps.append(step(.removeManagedTerminalProfile, "Remove managed Terminal Profile", false))
        }

        switch scope {
        case .alanIntegrationOnly:
            return ManagedTerminalAccountPlan(request: request, status: .readyToApply, steps: steps)
        case .deleteAccountAndHome(let confirmation):
            if state.account.requiresAlanManagedOwnership && !state.ownership.isAlanManaged {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .accountNotAlanManaged,
                    steps: steps
                )
            }
            guard confirmation == request.accountName else {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .requiresDestructiveConfirmation,
                    steps: steps
                )
            }
            var destructiveSteps: [ManagedTerminalAccountPlanStep] = []
            if canDeleteAccount(state: state) {
                destructiveSteps.append(step(.deleteAccount, "Delete terminal account", true))
            }
            if canDeleteHomeDirectory(request: request, state: state) {
                destructiveSteps.append(
                    step(.deleteHomeDirectory, "Delete terminal account home directory", true)
                )
            }
            return ManagedTerminalAccountPlan(
                request: request,
                status: .readyToApply,
                steps: steps + destructiveSteps
            )
        }
    }

    private static func canDeleteAccount(state: ManagedTerminalAccountState) -> Bool {
        switch state.account {
        case .standard, .admin:
            return true
        case .missing, .invalid:
            return false
        }
    }

    private static func canDeleteHomeDirectory(
        request: ManagedTerminalAccountRequest,
        state: ManagedTerminalAccountState
    ) -> Bool {
        guard request.homeDirectory == ManagedTerminalAccountRequest.canonicalHomeDirectory(
            for: request.accountName
        ) else {
            return false
        }
        switch state.account {
        case .standard(let homeDirectory, _, _), .admin(let homeDirectory, _, _):
            return homeDirectory == request.homeDirectory
        case .missing, .invalid:
            return false
        }
    }

    private static func step(
        _ kind: ManagedTerminalAccountPlanStepKind,
        _ summary: String,
        _ requiresPrivilege: Bool
    ) -> ManagedTerminalAccountPlanStep {
        ManagedTerminalAccountPlanStep(
            kind: kind,
            summary: summary,
            requiresPrivilege: requiresPrivilege
        )
    }

    private static func helperStep(
        _ kind: AlanManagedUserHelperPlanStepKind,
        _ summary: String
    ) -> ManagedTerminalAccountPlanStep {
        step(.helperStep(kind), summary, true)
    }

    private static func helperBackedSteps(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis
    ) -> [ManagedTerminalAccountPlanStep] {
        var steps: [ManagedTerminalAccountPlanStep] = []

        if !diagnosis.accountExists {
            steps.append(helperStep(.createStandardAccount, "Create standard local terminal account"))
        } else {
            if !diagnosis.homeDirectoryExists {
                steps.append(helperStep(.repairHomeDirectory, "Repair terminal account home directory"))
            }
            if !diagnosis.shellMatches {
                steps.append(helperStep(.repairShell, "Repair terminal account shell"))
            }
        }

        if request.hideFromLoginWindow && !diagnosis.hiddenFromLoginWindow {
            steps.append(helperStep(.hideAccount, "Hide terminal account from login window lists"))
        }
        if diagnosis.ownershipState != .alanManaged {
            steps.append(helperStep(.writeOwnershipMarker, "Write Alan-managed ownership marker"))
        }
        if shouldCleanupLegacySudoers(request: request, diagnosis: diagnosis) {
            steps.append(helperStep(.cleanupLegacySudoers, "Clean up verified legacy Alan sudoers"))
        }
        steps.append(helperStep(.verifyAccount, "Verify helper-managed account state"))
        if !diagnosis.ptySmokeVerified {
            steps.append(helperStep(.verifyManagedUserPTY, "Verify helper-managed PTY startup"))
        }
        steps.append(contentsOf: terminalProfileHandoffSteps(request: request, diagnosis: diagnosis))
        return steps
    }

    private static func terminalProfileHandoffSteps(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis
    ) -> [ManagedTerminalAccountPlanStep] {
        guard diagnosis.terminalProfileID == request.terminalProfileID else {
            return [
                step(.createOrUpdateTerminalProfile, "Create matching Terminal Profile", false),
            ]
        }
        return []
    }

    private static func shouldCleanupLegacySudoers(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis
    ) -> Bool {
        guard let path = diagnosis.legacySudoersPath else { return false }
        return path == ManagedTerminalAccountSudoersRule(request: request).filePath
    }

    private static func unexpectedLegacySudoersPath(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis
    ) -> String? {
        guard let path = diagnosis.legacySudoersPath else { return nil }
        return path == ManagedTerminalAccountSudoersRule(request: request).filePath ? nil : path
    }

    private static func shouldRemoveSudoersDropIn(
        request: ManagedTerminalAccountRequest,
        sudoers: ManagedTerminalAccountSudoersState
    ) -> Bool {
        let expectedPath = ManagedTerminalAccountSudoersRule(request: request).filePath
        switch sudoers {
        case .alanOwnedValid(let path), .existingUnreadable(let path):
            return path == expectedPath
        case .missing, .alanOwnedInvalid, .unmanaged:
            return false
        }
    }
}

struct ManagedTerminalAccountSudoersRule: Equatable {
    static let managedMarker = "# Managed by Alan for terminal account entry. Do not edit by hand."

    let request: ManagedTerminalAccountRequest

    var fileName: String {
        "alan-terminal-\(request.guiUserName)-to-\(request.accountName)"
    }

    var filePath: String {
        "/etc/sudoers.d/\(fileName)"
    }

    var contents: String {
        """
        \(Self.managedMarker)
        \(request.guiUserName) ALL=(\(request.accountName)) NOPASSWD: ALL
        """
    }
}

struct ManagedTerminalAccountApplyResult: Equatable {
    let completedSteps: [ManagedTerminalAccountPlanStepKind]
    let failedStep: ManagedTerminalAccountPlanStepKind?
    let cancelled: Bool
    let visibleDiagnostics: [String]

    static func cancelled(before steps: [ManagedTerminalAccountPlanStepKind]) -> ManagedTerminalAccountApplyResult {
        ManagedTerminalAccountApplyResult(
            completedSteps: [],
            failedStep: steps.first,
            cancelled: true,
            visibleDiagnostics: ["Provisioning cancelled before privileged changes."]
        )
    }
}

protocol ManagedTerminalAccountPrivilegedExecuting {
    func apply(_ plan: ManagedTerminalAccountPlan) -> ManagedTerminalAccountApplyResult
}

struct ManagedTerminalAccountLocalEffectResult: Equatable {
    let succeeded: Bool
    let redactedMessage: String

    static func succeeded(_ redactedMessage: String) -> ManagedTerminalAccountLocalEffectResult {
        ManagedTerminalAccountLocalEffectResult(succeeded: true, redactedMessage: redactedMessage)
    }

    static func failed(_ redactedMessage: String) -> ManagedTerminalAccountLocalEffectResult {
        ManagedTerminalAccountLocalEffectResult(succeeded: false, redactedMessage: redactedMessage)
    }
}

protocol ManagedTerminalAccountLocalEffectExecuting {
    func apply(
        _ step: ManagedTerminalAccountPlanStepKind,
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult?
}

struct ManagedTerminalAccountTerminalProfileEffectExecutor: ManagedTerminalAccountLocalEffectExecuting {
    let store: TerminalProfileStore
    let currentSpaceBinder: ((String) -> Bool)?

    init(
        store: TerminalProfileStore = .defaultStore(),
        currentSpaceBinder: ((String) -> Bool)? = nil
    ) {
        self.store = store
        self.currentSpaceBinder = currentSpaceBinder
    }

    func apply(
        _ step: ManagedTerminalAccountPlanStepKind,
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult? {
        switch step {
        case .createOrUpdateTerminalProfile:
            return createOrUpdateTerminalProfile(for: request)
        case .bindCurrentSpace:
            return bindCurrentSpace(to: request.terminalProfileID)
        case .removeManagedTerminalProfile:
            return removeManagedTerminalProfile(for: request)
        case .createStandardAccount, .repairAccountType, .repairHomeDirectory, .repairShell,
                .hideAccount, .writeSudoersDropIn, .validateSudoers, .verifyTerminalEntry,
                .removeSudoersDropIn, .deleteAccount, .deleteHomeDirectory, .helperStep:
            return nil
        }
    }

    private func createOrUpdateTerminalProfile(
        for request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult {
        let document = store.load().document
        let draft = TerminalProfileEditorDraft(
            id: request.terminalProfileID,
            title: request.fullName ?? request.accountName,
            launchKind: .managedUser,
            unixUser: request.accountName,
            customCommand: "",
            defaultWorkingDirectory: request.homeDirectory,
            presentation: TerminalProfilePresentation(
                symbolName: "person.crop.circle",
                colorName: nil
            ),
            managedTerminalAccountID: request.accountName
        )
        let editorResult = TerminalProfileEditor.upserting(draft: draft, into: document)
        guard let nextDocument = editorResult.document else {
            return .failed("Terminal Profile handoff failed. Credentials redacted.")
        }
        return save(
            nextDocument,
            successMessage: "Terminal Profile handoff completed. Credentials redacted.",
            failureMessage: "Terminal Profile handoff failed. Credentials redacted."
        )
    }

    private func bindCurrentSpace(to profileID: String) -> ManagedTerminalAccountLocalEffectResult {
        guard let currentSpaceBinder else {
            return .failed("Current Space binding is unavailable. Credentials redacted.")
        }
        guard currentSpaceBinder(profileID) else {
            return .failed("Current Space binding failed. Credentials redacted.")
        }
        return .succeeded("Current Space binding completed. Credentials redacted.")
    }

    private func removeManagedTerminalProfile(
        for request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult {
        let document = store.load().document
        let remainingProfiles = document.profiles.filter {
            $0.managedTerminalAccountID != request.accountName
        }

        guard remainingProfiles.count != document.profiles.count else {
            return .succeeded("Managed Terminal Profile was already absent. Credentials redacted.")
        }

        let nextDocument: TerminalProfileDocument
        if remainingProfiles.isEmpty {
            nextDocument = .fallback
        } else {
            let defaultProfileID = remainingProfiles.contains { $0.id == document.defaultProfileID }
                ? document.defaultProfileID
                : remainingProfiles[0].id
            nextDocument = TerminalProfileDocument(
                defaultProfileID: defaultProfileID,
                profiles: remainingProfiles
            )
        }

        return save(
            nextDocument,
            successMessage: "Managed Terminal Profile removal completed. Credentials redacted.",
            failureMessage: "Managed Terminal Profile removal failed. Credentials redacted."
        )
    }

    private func save(
        _ document: TerminalProfileDocument,
        successMessage: String,
        failureMessage: String
    ) -> ManagedTerminalAccountLocalEffectResult {
        do {
            try store.save(document)
            return .succeeded(successMessage)
        } catch {
            return .failed(failureMessage)
        }
    }
}

struct ManagedTerminalAccountHelperExecutor: ManagedTerminalAccountPrivilegedExecuting {
    let channel: AlanInstallChannel
    let helperClient: AlanPrivilegedHelperClienting
    let localEffectExecutor: ManagedTerminalAccountLocalEffectExecuting

    init(
        channel: AlanInstallChannel = .current(),
        helperClient: AlanPrivilegedHelperClienting,
        localEffectExecutor: ManagedTerminalAccountLocalEffectExecuting =
            ManagedTerminalAccountTerminalProfileEffectExecutor()
    ) {
        self.channel = channel
        self.helperClient = helperClient
        self.localEffectExecutor = localEffectExecutor
    }

    func apply(_ plan: ManagedTerminalAccountPlan) -> ManagedTerminalAccountApplyResult {
        if let rejectedStep = plan.steps.first(where: rejectsLegacyPrivilegedStep) {
            return ManagedTerminalAccountApplyResult(
                completedSteps: [],
                failedStep: rejectedStep.kind,
                cancelled: false,
                visibleDiagnostics: [
                    "Helper-backed Managed User plan rejected a legacy privileged step. Credentials redacted.",
                ]
            )
        }

        let helperPlan = AlanManagedUserHelperPlan(
            operationID: UUID().uuidString,
            channelID: channel.installChannelID,
            request: plan.request,
            steps: plan.steps.compactMap(helperPlanStep)
        )

        var completed: [ManagedTerminalAccountPlanStepKind] = []
        if !helperPlan.steps.isEmpty {
            let helperResult = helperClient.applyManagedUserPlan(helperPlan)
            completed.append(contentsOf: helperResult.completedSteps)
            if helperResult.failedStep != nil || helperResult.cancelled {
                return helperResult
            }
        }

        for step in plan.steps where !step.requiresPrivilege {
            guard let result = localEffectExecutor.apply(step.kind, request: plan.request) else {
                continue
            }
            guard result.succeeded else {
                return ManagedTerminalAccountApplyResult(
                    completedSteps: completed,
                    failedStep: step.kind,
                    cancelled: false,
                    visibleDiagnostics: [result.redactedMessage]
                )
            }
            completed.append(step.kind)
        }

        return ManagedTerminalAccountApplyResult(
            completedSteps: completed,
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Helper-backed Managed User plan applied. Credentials redacted."]
        )
    }

    private func rejectsLegacyPrivilegedStep(_ step: ManagedTerminalAccountPlanStep) -> Bool {
        guard step.requiresPrivilege else { return false }
        if case .helperStep = step.kind {
            return false
        }
        return true
    }

    private func helperPlanStep(
        _ step: ManagedTerminalAccountPlanStep
    ) -> AlanManagedUserHelperPlanStep? {
        guard case .helperStep(let kind) = step.kind else { return nil }
        return AlanManagedUserHelperPlanStep(
            kind: kind,
            summary: step.summary,
            requiresDestructiveConfirmation: kind == .deleteAccount || kind == .deleteHomeDirectory
        )
    }
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
