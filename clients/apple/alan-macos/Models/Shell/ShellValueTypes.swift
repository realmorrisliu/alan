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
    case customCommand = "custom_command"
}

enum TerminalProfileLaunch: Codable, Equatable {
    case loginShell
    case sudoUser(unixUser: String)
    case sudoRoot
    case customCommand(String)

    var kind: TerminalProfileLaunchKind {
        switch self {
        case .loginShell:
            return .loginShell
        case .sudoUser:
            return .sudoUser
        case .sudoRoot:
            return .sudoRoot
        case .customCommand:
            return .customCommand
        }
    }

    var unixUser: String? {
        guard case .sudoUser(let unixUser) = self else { return nil }
        return unixUser
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
        case .sudoUser(let unixUser):
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
    case missingDefaultProfile(String)
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
        case .missingDefaultProfile(let id):
            return "Default Terminal Profile \(id) is missing."
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

struct ManagedTerminalAccountRequest: Equatable {
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
            process.waitUntilExit()
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
    let terminalProfile: ManagedTerminalAccountProfileState
    let verification: ManagedTerminalAccountVerificationStatus
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

    init(
        fileManager: FileManager = .default,
        commandRunner: ManagedTerminalAccountCommandRunning = ManagedTerminalAccountProcessRunner(),
        sudoersSyntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking = ManagedTerminalAccountVisudoSyntaxChecker()
    ) {
        self.fileManager = fileManager
        self.commandRunner = commandRunner
        self.sudoersSyntaxChecker = sudoersSyntaxChecker
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

        return ManagedTerminalAccountState(
            account: accountRecord(for: request),
            sudoers: ManagedTerminalAccountSudoersValidator.state(
                request: request,
                fileManager: fileManager,
                syntaxChecker: sudoersSyntaxChecker
            ),
            terminalProfile: terminalProfileState(for: request, document: terminalProfiles),
            verification: .notRun
        )
    }

    private func accountRecord(for request: ManagedTerminalAccountRequest) -> ManagedTerminalAccountRecord {
        let dscl = commandRunner.run(
            executablePath: "/usr/bin/dscl",
            arguments: [
                ".",
                "-read",
                "/Users/\(request.accountName)",
                "NFSHomeDirectory",
                "UserShell",
                "IsHidden",
            ]
        )
        guard dscl.succeeded else {
            return .missing
        }

        let output = dscl.standardOutput
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
                let prefix = "\(key):"
                guard line.hasPrefix(prefix) else { return nil }
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
        if profile.managedTerminalAccountID == request.accountName,
           profile.launch == .sudoUser(unixUser: request.accountName)
        {
            if profile.defaultWorkingDirectory != request.homeDirectory {
                return .existingManagedOutdated(profileID: profile.id)
            }
            return .existingManaged(profileID: profile.id)
        }
        return .existingUnmanaged(profileID: profile.id)
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
            if shell != request.shell {
                return .failed(step: .shell, message: "Login shell does not match the plan.")
            }
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
            guard confirmation == request.accountName else {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .requiresDestructiveConfirmation,
                    steps: steps
                )
            }
            var destructiveSteps = [
                step(.deleteAccount, "Delete terminal account", true),
            ]
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

struct ManagedTerminalAccountPrivilegedCommandResult: Equatable {
    let succeeded: Bool
    let redactedMessage: String
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
                .removeSudoersDropIn, .deleteAccount, .deleteHomeDirectory:
            return nil
        }
    }

    private func createOrUpdateTerminalProfile(
        for request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult {
        let document = store.load().document
        let editorResult = TerminalProfileEditor.upserting(
            draft: TerminalProfileEditorDraft(
                id: request.terminalProfileID,
                title: request.fullName ?? request.accountName,
                launchKind: .sudoUser,
                unixUser: request.accountName,
                defaultWorkingDirectory: request.homeDirectory,
                presentation: TerminalProfilePresentation(
                    symbolName: "person.crop.circle",
                    colorName: nil
                ),
                managedTerminalAccountID: request.accountName
            ),
            into: document
        )
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

protocol ManagedTerminalAccountPrivilegedCommandRunning {
    func runPrivilegedShellScript(
        _ script: String,
        redactedDescription: String
    ) -> ManagedTerminalAccountPrivilegedCommandResult
}

struct ManagedTerminalAccountAppleScriptPrivilegeRunner: ManagedTerminalAccountPrivilegedCommandRunning {
    let commandRunner: ManagedTerminalAccountCommandRunning

    init(commandRunner: ManagedTerminalAccountCommandRunning = ManagedTerminalAccountProcessRunner()) {
        self.commandRunner = commandRunner
    }

    func runPrivilegedShellScript(
        _ script: String,
        redactedDescription: String
    ) -> ManagedTerminalAccountPrivilegedCommandResult {
        let result = commandRunner.run(
            executablePath: "/usr/bin/osascript",
            arguments: [
                "-e",
                "do shell script \(appleScriptStringLiteral(script)) with administrator privileges",
            ]
        )
        return ManagedTerminalAccountPrivilegedCommandResult(
            succeeded: result.succeeded,
            redactedMessage: result.succeeded
                ? "\(redactedDescription) completed."
                : "\(redactedDescription) failed. Credentials redacted."
        )
    }

    private func appleScriptStringLiteral(_ value: String) -> String {
        let escaped = value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
            .replacingOccurrences(of: "\n", with: "\\n")
        return "\"\(escaped)\""
    }
}

struct ManagedTerminalAccountAuthorizedScriptExecutor: ManagedTerminalAccountPrivilegedExecuting {
    let request: ManagedTerminalAccountRequest
    let commandRunner: ManagedTerminalAccountPrivilegedCommandRunning
    let localEffectExecutor: ManagedTerminalAccountLocalEffectExecuting
    let entryVerifier: ManagedTerminalAccountEntryVerifying

    init(
        request: ManagedTerminalAccountRequest,
        commandRunner: ManagedTerminalAccountPrivilegedCommandRunning =
            ManagedTerminalAccountAppleScriptPrivilegeRunner(),
        localEffectExecutor: ManagedTerminalAccountLocalEffectExecuting =
            ManagedTerminalAccountTerminalProfileEffectExecutor(),
        entryVerifier: ManagedTerminalAccountEntryVerifying =
            ManagedTerminalAccountSudoEntryVerifier()
    ) {
        self.request = request
        self.commandRunner = commandRunner
        self.localEffectExecutor = localEffectExecutor
        self.entryVerifier = entryVerifier
    }

    func apply(_ plan: ManagedTerminalAccountPlan) -> ManagedTerminalAccountApplyResult {
        var completed: [ManagedTerminalAccountPlanStepKind] = []
        var diagnostics: [String] = []

        for step in plan.steps {
            if step.kind == .verifyTerminalEntry {
                let verification = entryVerifier.verifyTerminalEntry(request: request)
                guard verification.isValid else {
                    diagnostics.append("\(step.summary) failed. Credentials redacted.")
                    return ManagedTerminalAccountApplyResult(
                        completedSteps: completed,
                        failedStep: step.kind,
                        cancelled: false,
                        visibleDiagnostics: diagnostics
                    )
                }
                diagnostics.append("\(step.summary) completed.")
                completed.append(step.kind)
                continue
            }
            guard let script = script(for: step.kind) else {
                guard let result = localEffectExecutor.apply(step.kind, request: request) else {
                    diagnostics.append("Step has no executor: \(step.summary).")
                    return ManagedTerminalAccountApplyResult(
                        completedSteps: completed,
                        failedStep: step.kind,
                        cancelled: false,
                        visibleDiagnostics: diagnostics
                    )
                }
                diagnostics.append(result.redactedMessage)
                guard result.succeeded else {
                    return ManagedTerminalAccountApplyResult(
                        completedSteps: completed,
                        failedStep: step.kind,
                        cancelled: false,
                        visibleDiagnostics: diagnostics
                    )
                }
                completed.append(step.kind)
                continue
            }
            let result = commandRunner.runPrivilegedShellScript(
                script,
                redactedDescription: step.summary
            )
            diagnostics.append(result.redactedMessage)
            guard result.succeeded else {
                return ManagedTerminalAccountApplyResult(
                    completedSteps: completed,
                    failedStep: step.kind,
                    cancelled: false,
                    visibleDiagnostics: diagnostics
                )
            }
            completed.append(step.kind)
        }

        return ManagedTerminalAccountApplyResult(
            completedSteps: completed,
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: diagnostics.isEmpty
                ? ["Provisioning plan completed. Credentials redacted."]
                : diagnostics
        )
    }

    private func script(for step: ManagedTerminalAccountPlanStepKind) -> String? {
        let account = shellQuote(request.accountName)
        let accountRecord = shellQuote("/Users/\(request.accountName)")
        let home = shellQuote(request.homeDirectory)
        let shell = shellQuote(request.shell)
        switch step {
        case .createStandardAccount:
            let fullName = shellQuote(request.fullName ?? request.accountName)
            let disabledAuthentication = shellQuote(";DisabledUser;")
            return """
            set -eu
            account_name=\(account)
            account_record=\(accountRecord)
            account_home=\(home)
            account_shell=\(shell)
            account_full_name=\(fullName)
            disabled_auth=\(disabledAuthentication)
            next_uid="$(
              /usr/bin/dscl . -list /Users UniqueID |
                /usr/bin/awk '$2 ~ /^[0-9]+$/ && $2 >= 501 { if ($2 >= max) max=$2 } END { print (max ? max + 1 : 501) }'
            )"
            while /usr/bin/dscl . -search /Users UniqueID "$next_uid" >/dev/null 2>&1; do
              next_uid=$((next_uid + 1))
            done
            /usr/bin/dscl . -create "$account_record"
            /usr/bin/dscl . -create "$account_record" UserShell "$account_shell"
            /usr/bin/dscl . -create "$account_record" RealName "$account_full_name"
            /usr/bin/dscl . -create "$account_record" NFSHomeDirectory "$account_home"
            /usr/bin/dscl . -create "$account_record" PrimaryGroupID 20
            /usr/bin/dscl . -create "$account_record" UniqueID "$next_uid"
            /usr/bin/dscl . -create "$account_record" GeneratedUID "$(/usr/bin/uuidgen)"
            /usr/bin/dscl . -create "$account_record" AuthenticationAuthority "$disabled_auth"
            /usr/sbin/createhomedir -c -u "$account_name" >/dev/null 2>&1 || true
            /usr/sbin/dseditgroup -o edit -d \(account) -t user admin >/dev/null 2>&1 || true
            """
        case .repairAccountType:
            return "/usr/sbin/dseditgroup -o edit -d \(account) -t user admin >/dev/null 2>&1 || true"
        case .repairHomeDirectory:
            return """
            /usr/bin/dscl . -create \(accountRecord) NFSHomeDirectory \(home)
            /usr/sbin/createhomedir -c -u \(account) >/dev/null 2>&1 || true
            """
        case .repairShell:
            return "/usr/bin/dscl . -create \(accountRecord) UserShell \(shell)"
        case .hideAccount:
            return "/usr/bin/dscl . -create \(accountRecord) IsHidden 1"
        case .writeSudoersDropIn:
            return sudoersWriteScript()
        case .validateSudoers:
            return "/usr/sbin/visudo -cf \(shellQuote(ManagedTerminalAccountSudoersRule(request: request).filePath))"
        case .verifyTerminalEntry:
            return nil
        case .removeSudoersDropIn:
            return sudoersRemoveScript()
        case .deleteAccount:
            return "/usr/sbin/sysadminctl -deleteUser \(account)"
        case .deleteHomeDirectory:
            return "/bin/rm -rf \(home)"
        case .createOrUpdateTerminalProfile, .bindCurrentSpace, .removeManagedTerminalProfile:
            return nil
        }
    }

    private func sudoersWriteScript() -> String {
        let rule = ManagedTerminalAccountSudoersRule(request: request)
        return """
        set -eu
        temp_dir="$(/usr/bin/mktemp -d /tmp/alan-terminal-sudoers.XXXXXXXXXX)"
        trap '/bin/rm -rf "$temp_dir"' 0 1 2 15
        temp_path="$temp_dir/sudoers"
        umask 077
        /bin/cat > "$temp_path" <<'ALAN_SUDOERS'
        \(rule.contents)
        ALAN_SUDOERS
        /usr/sbin/visudo -cf "$temp_path"
        /usr/bin/install -o root -g wheel -m 0440 "$temp_path" \(shellQuote(rule.filePath))
        """
    }

    private func sudoersRemoveScript() -> String {
        let rule = ManagedTerminalAccountSudoersRule(request: request)
        return """
        set -eu
        sudoers_path=\(shellQuote(rule.filePath))
        if [ ! -e "$sudoers_path" ] && [ ! -L "$sudoers_path" ]; then
          exit 0
        fi
        temp_dir="$(/usr/bin/mktemp -d /tmp/alan-terminal-sudoers-remove.XXXXXXXXXX)"
        trap '/bin/rm -rf "$temp_dir"' 0 1 2 15
        expected_path="$temp_dir/expected"
        umask 077
        /bin/cat > "$expected_path" <<'ALAN_SUDOERS'
        \(rule.contents)
        ALAN_SUDOERS
        if ! /usr/bin/cmp -s "$expected_path" "$sudoers_path"; then
          /bin/echo "Refusing to remove sudoers drop-in with unexpected contents." >&2
          exit 1
        fi
        /bin/rm -f "$sudoers_path"
        """
    }

    private func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
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
