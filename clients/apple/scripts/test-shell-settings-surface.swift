import Foundation

struct TerminalRenderCoordinatorMetrics: Codable, Equatable {}

#if os(macOS)
private enum TestFailure: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case .message(let message):
            return message
        }
    }
}

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() {
        throw TestFailure.message(message)
    }
}

@main
struct ShellSettingsSurfaceTestRunner {
    static func main() {
        do {
            try testDefaultSectionOrderAndInterfaceMutability()
            try testAccountsCapabilitiesAndLocalRowsStayReadOnlyAndRedacted()
            try testDevChannelLocalRowsUseDevIdentity()
            try testPrivilegedHelperIdentityIsChannelScoped()
            try testPrivilegedHelperCurrentIdentityUsesLaunchdServiceName()
            try testPrivilegedHelperLifecycleServiceUsesSMAppServiceIdentityAndFakeStates()
            try testPrivilegedHelperSettingsRowsExposeLifecycleStates()
            try testPrivilegedHelperXPCBoundaryIsTypedAndChannelScoped()
            try testPrivilegedHelperPtyInputPreservesShortWrites()
            try testPrivilegedHelperRequestValidationIsNarrowAndSanitized()
            try testManagedUserHelperBackedPathForbidsLegacyExecutorFallback()
            try testLocalSummaryReadsHostConfigForDaemonEndpoint()
            try testLocalFolderOpenerRequiresExistingDirectory()
            try testWorkspaceContextUsesRegistryForWorkspaceScopedRequests()
            try testWorkspaceContextFallsBackToDiscoveredWorkspaceRoot()
            try testUnavailableRemoteSummariesStayCompact()
            try testTerminalProfilesAndAccountsStayLocalAndRedacted()
            try testManagedUserTerminalProfilesUseHelperLaunchIdentity()
            try testManagedUserRowsExposeStateAppropriateActions()
            try testManagedUserRowsExposeHelperBackedStates()
            try testManagedUserCatalogIncludesPersistedAndIncompleteUsers()
            try testManagedUserExistingOrdinaryAccountReportsNotAlanManaged()
            try testManagedUserSummaryUsesHelperDiagnosisStates()
            try testManagedUserReviewPreviewUsesHelperOperationRows()
            try testManagedUserCreationPreviewDerivesDefaultsAndRejectsConflicts()
            try testManagedUserSummaryRunsReadinessVerificationBeforePlanning()
            try testManagedUserApplyUsesPrivilegedExecutorAndRefreshesStatus()
            try testManagedUserApplyUsesHelperDeclarativePlanAndRejectsLegacySteps()
            try testFakeHelperCoversManagedUserRemovalPtyAndDenialStates()
            try testManagedUserRollbackRequiresAlanOwnershipForDestructiveDeletion()
            try testManagedProfileReadinessFiltersSpaceIdentityChoices()
            try testPerformanceDiagnosticsRowsAreCompactAndLocal()
            try testNavigationGroupsMapTaskOrientedRows()
            try testNavigationGroupsKeepTerminalIdentityOutOfAgent()
            try testNavigationGroupsPlaceAgentAndSystemRows()
            print("Shell settings surface tests passed.")
        } catch {
            fputs("Shell settings surface tests failed: \(error)\n", stderr)
            exit(1)
        }
    }
}

private func testDefaultSectionOrderAndInterfaceMutability() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )

    try expect(
        snapshot.sections.map(\.id) == ShellSettingsSectionID.defaultOrder,
        "settings sections must render local Terminal Profiles before provider Accounts"
    )

    let interface = try requireSection(.interface, in: snapshot)
    try expect(
        interface.rows.map(\.id) == ["appearance", "sidebar", "inactiveSplitDimming"],
        "interface section must keep existing appearance/sidebar/dimming preferences"
    )
    try expect(
        interface.rows.allSatisfy { $0.mutability == .editable },
        "interface preferences must remain directly editable"
    )
    try expect(
        interface.rows.allSatisfy { row in
            guard let detail = row.detail?.trimmingCharacters(in: .whitespacesAndNewlines) else {
                return false
            }
            return !detail.isEmpty && detail.count <= 80
        },
        "interface preferences must include concise secondary copy for native settings rows"
    )
}

private func testAccountsCapabilitiesAndLocalRowsStayReadOnlyAndRedacted() throws {
    let account = ShellSettingsConnectionProfile(
        profileID: "openai-main",
        label: "Work account",
        provider: "openai_responses",
        credentialStatus: "available",
        settings: [
            "model": "gpt-5.3",
            "api_key": "sk-test-should-not-render",
            "refresh_token": "refresh-token-should-not-render",
        ],
        isDefault: true
    )
    let provider = ShellSettingsConnectionProvider(
        providerID: "openai_responses",
        displayName: "OpenAI Responses",
        supportsBrowserLogin: false,
        supportsDeviceLogin: false,
        supportsSecretEntry: true,
        supportsLogout: true,
        supportsTest: true
    )
    let accounts = ShellSettingsAccountsSummary(
        current: ShellSettingsConnectionSelection(
            defaultProfile: "openai-main",
            effectiveProfile: "openai-main",
            effectiveSource: "default_profile"
        ),
        profiles: [account],
        providers: [provider],
        unavailableReason: nil
    )
    let capabilities = ShellSettingsCapabilitiesSummary(
        skills: [
            ShellSettingsSkillSummary(
                id: "memory",
                name: "Memory",
                enabled: true,
                allowImplicitInvocation: false,
                available: true
            ),
            ShellSettingsSkillSummary(
                id: "plan",
                name: "Plan",
                enabled: false,
                allowImplicitInvocation: false,
                available: true
            ),
        ],
        unavailableReason: nil
    )
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: ShellSettingsRemoteSnapshot(accounts: accounts, capabilities: capabilities),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let visibleText = snapshot.visibleText.joined(separator: "\n")

    try expect(
        !visibleText.contains("sk-test-should-not-render"),
        "settings must not render API key values"
    )
    try expect(
        !visibleText.contains("refresh-token-should-not-render"),
        "settings must not render refresh token values"
    )
    try expect(
        !visibleText.contains("api_key") && !visibleText.contains("refresh_token"),
        "settings must not render raw secret setting names"
    )
    try expect(
        !visibleText.localizedCaseInsensitiveContains("mount"),
        "capabilities must use enabled and implicit-invocation terminology instead of mount labels"
    )

    for sectionID in [ShellSettingsSectionID.accounts, .capabilities] {
        let section = try requireSection(sectionID, in: snapshot)
        try expect(
            section.rows.allSatisfy { $0.mutability != .editable },
            "\(sectionID.rawValue) rows must not be directly editable in the first settings phase"
        )
        try expect(
            section.rows.allSatisfy { !$0.offersFreeformEditing },
            "\(sectionID.rawValue) rows must not expose freeform file or credential editing"
        )
    }

    let localSection = try requireSection(.local, in: snapshot)
    try expect(
        localSection.rows.filter { $0.mutability == .editable }.map(\.id) == ["performanceDiagnostics"],
        "local settings must keep only the performance diagnostics toggle editable"
    )
    try expect(
        localSection.rows.allSatisfy { !$0.offersFreeformEditing },
        "local rows must not expose freeform file or credential editing"
    )
}

private func testDevChannelLocalRowsUseDevIdentity() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: devLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let localText = try requireSection(.local, in: snapshot).visibleText.joined(separator: "\n")

    try expect(
        localText.contains("app.alanworks.macos.dev"),
        "dev settings must show the Alan Dev bundle identifier"
    )
    try expect(localText.contains("alan-dev"), "dev settings must show the alan-dev CLI tool")
    try expect(localText.contains("~/.alan-dev"), "dev settings must show the dev alan home")
    try expect(
        localText.contains("~/.agents-dev/skills"),
        "dev settings must show the dev public skill root"
    )
    try expect(
        localText.contains("127.0.0.1:8091"),
        "dev settings must show the dev daemon bind or endpoint"
    )
    try expect(
        localText.contains("alan-dev-shell-control"),
        "dev settings must show the dev shell-control namespace"
    )
}

private func testPrivilegedHelperIdentityIsChannelScoped() throws {
    let stable = AlanInstallChannel.stable.privilegedHelperIdentity(
        signingTeamIdentifier: "TEAMID1234"
    )
    let dev = AlanInstallChannel.dev.privilegedHelperIdentity(
        signingTeamIdentifier: "TEAMID1234"
    )

    try expect(
        stable.registrationAPI == .smAppServiceDaemon && dev.registrationAPI == .smAppServiceDaemon,
        "helper registration must use SMAppService daemon registration"
    )
    try expect(
        stable.helperBundleIdentifier == "app.alanworks.macos.privileged-helper",
        "stable helper bundle identifier must be stable-channel scoped"
    )
    try expect(
        dev.helperBundleIdentifier == "app.alanworks.macos.dev.privileged-helper",
        "dev helper bundle identifier must be dev-channel scoped without a duplicate dev suffix"
    )
    try expect(
        stable.launchdServiceLabel != dev.launchdServiceLabel
            && stable.machServiceName != dev.machServiceName
            && stable.dataRootPath != dev.dataRootPath,
        "stable and dev helpers must use separate service labels, Mach services, and data roots"
    )
    try expect(
        stable.expectedClientRequirement
            == #"anchor apple generic and identifier "app.alanworks.macos" and certificate leaf[subject.OU] = "TEAMID1234""#
            && dev.expectedClientRequirement
            == #"anchor apple generic and identifier "app.alanworks.macos.dev" and certificate leaf[subject.OU] = "TEAMID1234""#,
        "helper client requirements must be scoped to the matching app bundle and signing team"
    )
}

private func testPrivilegedHelperCurrentIdentityUsesLaunchdServiceName() throws {
    let identity = AlanPrivilegedHelperXPCIdentity.current(
        bundleIdentifier: nil,
        environment: ["XPC_SERVICE_NAME": "app.alanworks.macos.dev.privileged-helper"],
        executablePath: "/Users/morris/Applications/Alan Dev.app/Contents/Library/LaunchServices/app.alanworks.macos.dev.privileged-helper",
        signingTeamIdentifier: "TEAMID1234"
    )
    try expect(
        identity.channelID == "dev"
            && identity.helperBundleIdentifier == "app.alanworks.macos.dev.privileged-helper"
            && identity.machServiceName == "app.alanworks.macos.dev.privileged-helper.xpc",
        "privileged helper must derive the dev identity from launchd service name when Bundle.main.bundleIdentifier is unavailable"
    )
    try expect(
        identity.expectedClientRequirement.contains(#"certificate leaf[subject.OU] = "TEAMID1234""#),
        "privileged helper runtime identity must include the signing team requirement"
    )

    let stableIdentity = AlanPrivilegedHelperXPCIdentity.current(
        bundleIdentifier: nil,
        environment: ["XPC_SERVICE_NAME": "app.alanworks.macos.privileged-helper.xpc"],
        executablePath: nil,
        signingTeamIdentifier: "TEAMID1234"
    )
    try expect(
        stableIdentity.channelID == "stable"
            && stableIdentity.helperBundleIdentifier == "app.alanworks.macos.privileged-helper"
            && stableIdentity.machServiceName == "app.alanworks.macos.privileged-helper.xpc",
        "privileged helper must normalize launchd Mach service names back to the helper label"
    )
}

private func testPrivilegedHelperLifecycleServiceUsesSMAppServiceIdentityAndFakeStates() throws {
    let liveManager = AlanPrivilegedHelperAppServiceManager(channel: .dev)
    try expect(
        liveManager.identity.plistName == "app.alanworks.macos.dev.privileged-helper.plist",
        "live helper manager must use the channel-scoped SMAppService plist name"
    )
    try expect(
        liveManager.identity.machServiceName == "app.alanworks.macos.dev.privileged-helper.xpc",
        "live helper manager must use the channel-scoped Mach service"
    )

    let fake = AlanPrivilegedHelperFakeLifecycleManager(channel: .dev)
    try expect(fake.status().state == .notInstalled, "fake helper must start from the requested state")
    try expect(
        fake.installOrUpdate().status.state == .healthy,
        "fake helper install must transition to healthy"
    )
    try expect(
        fake.installOrUpdate().action == .update,
        "fake helper installOrUpdate must report update once already healthy"
    )
    try expect(
        fake.validateSignature().state == .healthy,
        "fake helper signature validation must return the current helper status"
    )
    try expect(
        fake.uninstall().status.state == .notInstalled,
        "fake helper uninstall must transition to not installed"
    )
    try expect(
        fake.performedActions == [.install, .update, .validateSignature, .uninstall],
        "fake helper must record lifecycle actions in order"
    )

    let denied = AlanPrivilegedHelperFakeLifecycleManager(
        channel: .dev,
        deniedActions: [.install]
    ).installOrUpdate()
    try expect(!denied.succeeded, "fake helper must model lifecycle denial")
    try expect(
        denied.diagnostic?.code == .helperUnavailable,
        "fake helper denial must use a typed sanitized diagnostic"
    )
    try expect(
        denied.diagnostic?.sanitizedMessage.contains("do shell script") != true
            && denied.diagnostic?.sanitizedMessage.contains("NOPASSWD") != true,
        "fake helper denial must not expose raw privileged payloads"
    )
}

private func testPrivilegedHelperSettingsRowsExposeLifecycleStates() throws {
    let expectations: [(AlanPrivilegedHelperStatusState, String, [ShellSettingsRowActionKind])] = [
        (.notInstalled, "Not installed", [.installHelper]),
        (.outdated, "Outdated", [.updateHelper]),
        (.invalidSignature, "Invalid signature", [.updateHelper]),
        (.installing, "Installing", []),
        (.updating, "Updating", []),
        (.healthy, "Healthy", []),
        (.unavailable, "Unavailable", [.installHelper]),
        (.uninstallable, "Uninstallable", [.uninstallHelper]),
    ]

    for (state, value, actions) in expectations {
        let snapshot = ShellSettingsSurfaceSnapshot.make(
            remote: .unavailable(reason: "Daemon unavailable"),
            local: stableLocalSummary(),
            terminalProfiles: testTerminalProfiles(),
            privilegedHelper: PrivilegedHelperSettingsSummary(
                status: helperStatus(state: state)
            )
        )
        let accountSection = try requireSection(.terminalAccounts, in: snapshot)
        let helperRow = accountSection.rows.first { $0.id == "terminalPrivilegedHelper" }

        try expect(
            helperRow?.title == "Privileged helper",
            "Settings must expose a dedicated privileged helper status row"
        )
        try expect(
            helperRow?.value == value,
            "Helper state \(state.rawValue) must map to stable Settings value \(value)"
        )
        try expect(
            helperRow?.actions.map(\.id) == actions,
            "Helper state \(state.rawValue) must expose the expected lifecycle action"
        )
        try expect(
            helperRow?.detail?.contains("do shell script") != true
                && helperRow?.detail?.contains("NOPASSWD") != true
                && helperRow?.detail?.contains("/etc/sudoers") != true,
            "Helper status row must not expose raw privileged implementation details"
        )
    }
}

private func testPrivilegedHelperXPCBoundaryIsTypedAndChannelScoped() throws {
    let identity = AlanInstallChannel.dev.privilegedHelperIdentity(
        signingTeamIdentifier: "TEAMID1234"
    ).xpcIdentity
    let request = AlanPrivilegedHelperXPCRequest.helperStatus(
        identity: identity,
        operationID: "op-xpc-status"
    )
    let response = try invokeXPCStatus(
        service: AlanPrivilegedHelperXPCService(identity: identity),
        request: request
    )
    try expect(response.accepted, "helper XPC status request must be accepted for the matching channel")
    try expect(
        response.operationID == "op-xpc-status"
            && response.operation == .helperStatus
            && response.channelID == "dev",
        "helper XPC response must preserve typed operation metadata"
    )
    try expect(
        response.errorCode == nil
            && !response.sanitizedMessage.contains("sudo")
            && !response.sanitizedMessage.contains("do shell script"),
        "helper XPC success response must stay sanitized"
    )

    let stableIdentity = AlanInstallChannel.stable.privilegedHelperIdentity(
        signingTeamIdentifier: "TEAMID1234"
    ).xpcIdentity
    let mismatch = try invokeXPCStatus(
        service: AlanPrivilegedHelperXPCService(identity: identity),
        request: .helperStatus(identity: stableIdentity, operationID: "op-wrong-channel")
    )
    try expect(!mismatch.accepted, "helper XPC must reject channel-mismatched requests")
    try expect(
        mismatch.errorCode == .channelMismatch,
        "helper XPC channel mismatch must use a typed rejection code"
    )

    let invalidDataResponse = try invokeXPCStatus(
        service: AlanPrivilegedHelperXPCService(identity: identity),
        rawRequest: Data("not json".utf8) as NSData
    )
    try expect(
        invalidDataResponse.errorCode == .invalidRequest,
        "helper XPC must reject undecodable requests before privileged work"
    )

    let invalidManagedUser = ManagedTerminalAccountRequest(
        accountName: "bad user",
        guiUserName: "morris",
        fullName: "Bad User",
        shell: "/bin/bash",
        homeDirectory: "/tmp/bad user"
    )
    let diagnosisResponse = try invokeXPCPerform(
        service: AlanPrivilegedHelperXPCService(identity: identity),
        request: .operation(
            .diagnoseManagedUser,
            identity: identity,
            payload: try JSONEncoder().encode(invalidManagedUser),
            operationID: "op-diagnose-invalid"
        )
    )
    let diagnosis: AlanManagedUserDiagnosis = try decodedPayload(diagnosisResponse)
    try expect(
        diagnosis.diagnostic?.code == .invalidAccountIdentifier,
        "helper XPC diagnose must return typed validation diagnostics"
    )

    let invalidPlan = AlanManagedUserHelperPlan(
        operationID: "op-apply-invalid",
        channelID: "dev",
        request: invalidManagedUser,
        steps: [
            AlanManagedUserHelperPlanStep(
                kind: .createStandardAccount,
                summary: "Create invalid account",
                requiresDestructiveConfirmation: false
            ),
        ]
    )
    let applyResponse = try invokeXPCPerform(
        service: AlanPrivilegedHelperXPCService(identity: identity),
        request: .operation(
            .applyManagedUserPlan,
            identity: identity,
            payload: try JSONEncoder().encode(invalidPlan),
            operationID: "op-apply-invalid"
        )
    )
    let applyResult: AlanPrivilegedHelperXPCApplyResultPayload = try decodedPayload(applyResponse)
    try expect(
        !applyResponse.accepted
            && applyResult.failedHelperStep == AlanManagedUserHelperPlanStepKind.createStandardAccount.rawValue,
        "helper XPC apply must reject invalid declarative plans before system changes"
    )

    let startResponse = try invokeXPCPerform(
        service: AlanPrivilegedHelperXPCService(identity: identity),
        request: .operation(
            .startManagedUserPTY,
            identity: identity,
            payload: try JSONEncoder().encode(
                AlanManagedUserPTYStartRequest(
                    operationID: "op-start-invalid",
                    channelID: "dev",
                    accountName: "bad user",
                    homeDirectory: "/tmp/bad user",
                    shell: "/bin/bash",
                    contentID: "content-invalid",
                    columns: 80,
                    rows: 24
                )
            ),
            operationID: "op-start-invalid"
        )
    )
    let startDiagnostic: AlanPrivilegedHelperDiagnostic = try decodedPayload(startResponse)
    try expect(
        !startResponse.accepted && startDiagnostic.code == .invalidAccountIdentifier,
        "helper XPC PTY start must reject invalid account payloads before spawning"
    )

    let rawFailure = AlanPrivilegedHelperXPCResponse.rejected(
        request: request,
        identity: identity,
        code: .invalidRequest,
        message: "do shell script sudo -n -iu lab with password and /etc/sudoers NOPASSWD transcript"
    )
    let event = AlanPrivilegedHelperSanitizedEvent(response: rawFailure)
    try expect(
        !event.sanitizedMessage.contains("do shell script")
            && !event.sanitizedMessage.contains("sudo -n -iu")
            && !event.sanitizedMessage.contains("/etc/sudoers")
            && !event.sanitizedMessage.contains("NOPASSWD")
            && !event.sanitizedMessage.contains("password")
            && !event.sanitizedMessage.contains("transcript"),
        "helper sanitized events must exclude raw commands, sudoers, credentials, and transcripts"
    )

    let checker = AlanPrivilegedHelperFakeRequirementChecker(acceptedProcessIdentifiers: [123])
    switch checker.validateClient(processIdentifier: 123, expectedRequirement: identity.expectedClientRequirement) {
    case .success:
        break
    case .failure:
        throw TestFailure.message("fake code-signing checker must accept configured client pid")
    }
    switch checker.validateClient(processIdentifier: 124, expectedRequirement: identity.expectedClientRequirement) {
    case .success:
        throw TestFailure.message("fake code-signing checker must reject unconfigured client pid")
    case .failure(let code):
        try expect(
            code == .clientRequirementFailed,
            "fake code-signing checker must use the typed client-requirement failure"
        )
    }
}

private func testPrivilegedHelperPtyInputPreservesShortWrites() throws {
    let helperSource = try readRepositoryFile(
        "clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPC.swift"
    )
    let sessionStore = try sourceSlice(
        named: "private final class AlanPrivilegedHelperPTYSessionStore",
        in: helperSource,
        endingBefore: "private final class AlanPrivilegedHelperPTYSession"
    )

    try expect(
        sessionStore.contains("pendingInput.append(data)")
            && sessionStore.contains("drainPendingInput(session)")
            && sessionStore.contains("while !session.pendingInput.isEmpty")
            && sessionStore.contains("session.pendingInput.removeFirst(written)")
            && sessionStore.contains("errno == EAGAIN || errno == EWOULDBLOCK"),
        "helper PTY input must enqueue text and preserve unwritten suffixes after short nonblocking writes"
    )
}

private func testPrivilegedHelperRequestValidationIsNarrowAndSanitized() throws {
    let valid = ManagedTerminalAccountRequest(
        accountName: "lab",
        guiUserName: "morris",
        fullName: "Lab User"
    )
    try expect(
        AlanPrivilegedHelperRequestValidator.validate(request: valid, channel: .dev).isEmpty,
        "valid helper requests must pass the narrow account/home/shell contract"
    )

    let invalid = ManagedTerminalAccountRequest(
        accountName: "bad user",
        guiUserName: "morris",
        fullName: "Bad User",
        shell: "/bin/bash",
        homeDirectory: "/tmp/bad user"
    )
    let invalidCodes = Set(AlanPrivilegedHelperRequestValidator.validate(request: invalid, channel: .dev))
    try expect(
        invalidCodes == [.invalidAccountIdentifier, .invalidHomePath, .shellNotAllowed],
        "helper validation must reject invalid identifiers, non-canonical homes, and non-allowlisted shells"
    )
    try expect(
        AlanPrivilegedHelperRequestValidator.rejectsRawPrivilegedPayload(
            #"do shell script "dscl ." with administrator privileges"#
        ) == .rawCommandRejected,
        "helper validation must reject AppleScript privileged shell payloads"
    )
    try expect(
        AlanPrivilegedHelperRequestValidator.rejectsRawPrivilegedPayload("sudo -n -iu lab true")
            == .rawCommandRejected,
        "helper validation must reject sudo-based readiness fallback payloads"
    )
    try expect(
        AlanPrivilegedHelperRequestValidator.rejectsRawPrivilegedPayload(
            "/etc/sudoers.d/alan-terminal-morris-to-lab NOPASSWD: ALL"
        ) == .rawSudoersRejected,
        "helper validation must reject raw sudoers fragments"
    )

    let fake = AlanPrivilegedHelperFakeClient(channel: .dev, statusState: .invalidSignature)
    try expect(
        fake.status().state == .invalidSignature,
        "fake helper must expose invalid-signature status for code-signing denial paths"
    )
    let denied = fake.applyManagedUserPlan(
        AlanManagedUserHelperPlan(
            operationID: "op-test",
            channelID: "dev",
            request: valid,
            steps: [
                AlanManagedUserHelperPlanStep(
                    kind: .repairHomeDirectory,
                    summary: "Repair home",
                    requiresDestructiveConfirmation: false
                ),
            ]
        )
    )
    let diagnostics = denied.visibleDiagnostics.joined(separator: "\n")
    try expect(
        denied.failedStep == .helperStep(.repairHomeDirectory),
        "fake helper denial must preserve the typed helper step that failed"
    )
    try expect(
        !diagnostics.contains("NOPASSWD")
            && !diagnostics.contains("do shell script")
            && !diagnostics.contains("/etc/sudoers"),
        "helper denial diagnostics must stay sanitized"
    )
}

private func testManagedUserHelperBackedPathForbidsLegacyExecutorFallback() throws {
    let terminalPane = try readRepositoryFile("clients/apple/alan-macos/TerminalPaneView.swift")
    let shellValues = try readRepositoryFile("clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift")
    let helperExecutor = try sourceSlice(
        named: "struct ManagedTerminalAccountHelperExecutor",
        in: shellValues,
        endingBefore: "enum ShellTabActiveTaskState"
    )

    try expect(
        !terminalPane.contains("ManagedTerminalAccountAuthorizedScriptExecutor"),
        "Settings Managed User apply must not instantiate the legacy authorized script executor"
    )
    try expect(
        !shellValues.contains("ManagedTerminalAccountAuthorizedScriptExecutor")
            && !shellValues.contains("ManagedTerminalAccountAppleScriptPrivilegeRunner")
            && !shellValues.contains("with administrator privileges")
            && !shellValues.contains("/usr/bin/osascript"),
        "production Managed User code must not define the old osascript privileged executor"
    )
    for forbidden in [
        "writeSudoersDropIn",
        "validateSudoers",
        "verifyTerminalEntry",
        "sudo -n -iu",
        "do shell script",
        "/etc/sudoers.d/",
    ] {
        try expect(
            !helperExecutor.contains(forbidden),
            "helper-backed Managed User executor must not reference \(forbidden)"
        )
    }
}

private func testLocalSummaryReadsHostConfigForDaemonEndpoint() throws {
    let homeDirectory = try makeTemporaryDirectory()
    defer { try? FileManager.default.removeItem(at: homeDirectory) }

    let alanHome = homeDirectory.appendingPathComponent(".alan-dev", isDirectory: true)
    try FileManager.default.createDirectory(at: alanHome, withIntermediateDirectories: true)
    try """
    bind_address = "127.0.0.1:9123"
    """
    .write(to: alanHome.appendingPathComponent("host.toml"), atomically: true, encoding: .utf8)

    let summary = ShellSettingsLocalSummary.current(
        channel: .dev,
        environment: [:],
        updateDecision: unsupportedDevUpdateDecision(),
        homeDirectory: homeDirectory
    )

    try expect(
        summary.daemonBindAddress == "127.0.0.1:9123",
        "settings must display the bind address from the channel host.toml"
    )
    try expect(
        summary.daemonURL == "http://127.0.0.1:9123",
        "settings must query the daemon URL derived from host.toml"
    )
}

private func testLocalFolderOpenerRequiresExistingDirectory() throws {
    let homeDirectory = try makeTemporaryDirectory()
    defer { try? FileManager.default.removeItem(at: homeDirectory) }

    let existingDirectory = homeDirectory.appendingPathComponent("skills", isDirectory: true)
    try FileManager.default.createDirectory(at: existingDirectory, withIntermediateDirectories: true)
    let existingFile = homeDirectory.appendingPathComponent("not-a-folder")
    try Data("not a directory".utf8).write(to: existingFile)
    let missingDirectory = homeDirectory.appendingPathComponent("missing", isDirectory: true)

    try expect(
        ShellLocalFolderOpener.canOpenFolder(displayPath: existingDirectory.path),
        "folder opener must enable existing absolute directories"
    )
    try expect(
        !ShellLocalFolderOpener.canOpenFolder(displayPath: missingDirectory.path),
        "folder opener must disable missing absolute directories"
    )
    try expect(
        !ShellLocalFolderOpener.canOpenFolder(displayPath: existingFile.path),
        "folder opener must disable regular files"
    )
    try expect(
        !ShellLocalFolderOpener.canOpenFolder(displayPath: "relative/folder"),
        "folder opener must disable relative paths"
    )
    try expect(
        !ShellLocalFolderOpener.canOpenFolder(displayPath: "  "),
        "folder opener must disable blank paths"
    )
}

private func testWorkspaceContextUsesRegistryForWorkspaceScopedRequests() throws {
    let homeDirectory = try makeTemporaryDirectory()
    defer { try? FileManager.default.removeItem(at: homeDirectory) }

    let workspace = homeDirectory.appendingPathComponent("repo", isDirectory: true)
    let nestedDirectory = workspace.appendingPathComponent("Sources", isDirectory: true)
    try FileManager.default.createDirectory(at: nestedDirectory, withIntermediateDirectories: true)

    let alanHome = homeDirectory.appendingPathComponent(".alan-dev", isDirectory: true)
    try FileManager.default.createDirectory(at: alanHome, withIntermediateDirectories: true)
    let registry: [String: Any] = [
        "version": 1,
        "workspaces": [
            [
                "id": "abc123",
                "path": workspace.standardizedFileURL.path,
                "alias": "repo",
                "created_at": "2026-05-27T00:00:00Z",
            ],
        ],
    ]
    let registryData = try JSONSerialization.data(withJSONObject: registry, options: [.prettyPrinted])
    try registryData.write(to: alanHome.appendingPathComponent("registry.json"))

    let context = ShellSettingsWorkspaceContext.resolve(
        activeWorkingDirectory: nestedDirectory.path,
        channel: .dev,
        homeDirectory: homeDirectory
    )

    try expect(
        context.connectionWorkspaceDir == workspace.standardizedFileURL.path,
        "connection state must use the registered workspace root, not a nested terminal cwd"
    )
    try expect(
        context.skillCatalogWorkspaceDir == "repo",
        "skill catalog requests must use a registered workspace alias or short id"
    )
}

private func testWorkspaceContextFallsBackToDiscoveredWorkspaceRoot() throws {
    let homeDirectory = try makeTemporaryDirectory()
    defer { try? FileManager.default.removeItem(at: homeDirectory) }

    let workspace = homeDirectory.appendingPathComponent("unregistered", isDirectory: true)
    let nestedDirectory = workspace.appendingPathComponent("Sources", isDirectory: true)
    try FileManager.default.createDirectory(
        at: workspace.appendingPathComponent(".alan", isDirectory: true),
        withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(at: nestedDirectory, withIntermediateDirectories: true)

    let context = ShellSettingsWorkspaceContext.resolve(
        activeWorkingDirectory: nestedDirectory.path,
        channel: .dev,
        homeDirectory: homeDirectory
    )

    try expect(
        context.connectionWorkspaceDir == workspace.standardizedFileURL.path,
        "connection state must fall back to the discovered workspace root"
    )
    try expect(
        context.skillCatalogWorkspaceDir == nil,
        "unregistered workspaces must not be sent to the skill catalog alias-only endpoint"
    )
    try expect(
        context.skillCatalogUnavailableReason == "Register this workspace to show workspace skills.",
        "unregistered workspaces with .alan state must not fall back to the default skill catalog"
    )
}

private func testUnavailableRemoteSummariesStayCompact() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Connection refused"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let text = snapshot.visibleText.joined(separator: "\n")

    try expect(text.contains("Unavailable"), "unavailable remote sources must render a compact state")
    try expect(!text.contains("Error("), "unavailable state must not render raw debug payloads")
    try expect(
        !text.contains("thinking_budget_tokens"),
        "sessions summary must not expose deprecated thinking budget controls"
    )
}

private func testTerminalProfilesAndAccountsStayLocalAndRedacted() throws {
    let accountPlans = [
        ManagedTerminalAccountPlanner.plan(
            request: ManagedTerminalAccountRequest(
                accountName: "alan",
                guiUserName: "morris",
                fullName: "Alan Terminal"
            ),
            state: ManagedTerminalAccountState(
                account: .standard(homeDirectory: "/Users/alan", shell: "/bin/zsh", hidden: true),
                sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-alan"),
                ownership: alanManagedOwnership("alan"),
                terminalProfile: .existingManaged(profileID: "alan"),
                verification: .passed
            )
        ),
        ManagedTerminalAccountPlanner.plan(
            request: ManagedTerminalAccountRequest(
                accountName: "lab",
                guiUserName: "morris",
                fullName: "Lab User"
            ),
            state: ManagedTerminalAccountState(
                account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
                sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-lab"),
                ownership: alanManagedOwnership("lab"),
                terminalProfile: .existingUnmanaged(profileID: "lab"),
                verification: .passed
            )
        ),
    ]
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary(plans: accountPlans)
    )
    let profileSection = try requireSection(.terminalProfiles, in: snapshot)
    let accountSection = try requireSection(.terminalAccounts, in: snapshot)
    let providerSection = try requireSection(.accounts, in: snapshot)
    let visibleText = snapshot.visibleText.joined(separator: "\n")

    try expect(
        profileSection.visibleText.contains("Terminal Profiles"),
        "Terminal Profiles must render as local startup configuration"
    )
    try expect(
        accountSection.title == "Managed Users",
        "Settings must present managed terminal accounts as Managed Users"
    )
    try expect(
        profileSection.rows.first { $0.id == "terminalProfilesDefault" }?.title == "Default profile",
        "Terminal Profiles must expose the shell-core default profile control row"
    )
    try expect(
        profileSection.rows.first { $0.id == "terminalProfile.login_shell" }?.value == "login_shell",
        "Login shell profile row must preserve the shell-core launch kind value when it is not default"
    )
    try expect(
        accountSection.rows.contains { $0.title == "Alan Terminal" && $0.detail?.contains("alan") == true },
        "Managed Users must list each user by display label and Unix user name"
    )
    try expect(
        accountSection.rows.contains { $0.title == "Lab User" && $0.value == "Conflict" },
        "Managed Users must show independent conflict state for each user"
    )
    try expect(
        !providerSection.visibleText.contains("Alan"),
        "Terminal Profiles must stay out of provider Accounts"
    )
    try expect(
        !visibleText.contains("echo hidden-secret"),
        "Settings must not expose full custom command text in normal rows"
    )
    try expect(
        !visibleText.lowercased().contains("autologin"),
        "Settings copy must not use autologin wording"
    )
    try expect(
        accountSection.visibleText.joined(separator: " ").contains("terminal entry"),
        "Managed Terminal Account rows must describe terminal entry"
    )
}

private func testManagedUserTerminalProfilesUseHelperLaunchIdentity() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "lab",
        guiUserName: "morris",
        fullName: "Lab User"
    )
    let profile = ManagedTerminalAccountProfileHandoff.profileDefinition(
        for: request,
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
            sudoers: .missing,
            ownership: alanManagedOwnership("lab"),
            terminalProfile: .missing,
            verification: .passed
        )
    )
    try expect(
        profile?.launch == .managedUser(unixUser: "lab")
            && profile?.managedTerminalAccountID == "lab",
        "Managed User profile handoff must create managed_user profiles with ownership evidence"
    )

    let managedDocument = TerminalProfileDocument(
        defaultProfileID: "lab",
        profiles: [
            TerminalProfileDefinition.loginShellFallback,
            TerminalProfileDefinition(
                id: "lab",
                title: "Lab User",
                launch: .managedUser(unixUser: "lab"),
                defaultWorkingDirectory: "/Users/lab",
                presentation: nil,
                managedTerminalAccountID: "lab"
            ),
        ]
    )
    try expect(
        TerminalProfileValidator.validate(managedDocument).isValid,
        "managed_user profiles with matching Managed User ownership must validate"
    )
    let managedProfileRows = try requireSection(
        .terminalProfiles,
        in: ShellSettingsSurfaceSnapshot.make(
            remote: .unavailable(reason: "Daemon unavailable"),
            local: stableLocalSummary(),
            terminalProfiles: TerminalProfileSettingsSummary(
                profiles: managedDocument.profiles,
                defaultProfileID: managedDocument.defaultProfileID,
                recoveryMessage: nil
            ),
            managedTerminalAccounts: ManagedTerminalAccountSettingsSummary(
                plans: [
                    ManagedTerminalAccountPlanner.plan(
                        request: request,
                        state: ManagedTerminalAccountState(
                            account: .standard(
                                homeDirectory: "/Users/lab",
                                shell: "/bin/zsh",
                                hidden: true
                            ),
                            sudoers: .missing,
                            ownership: alanManagedOwnership("lab"),
                            terminalProfile: .existingManaged(profileID: "lab"),
                            verification: .passed
                        )
                    ),
                ]
            )
        )
    ).rows
    let managedProfileRow = managedProfileRows.first { $0.id == "terminalProfile.lab" }
    try expect(
        managedProfileRow?.mutability == .readOnly
            && managedProfileRow?.actions.isEmpty == true
            && managedProfileRow?.value == "Managed",
        "managed Terminal Profiles must stay read-only in the profile editor"
    )

    let missingManagedAccount = TerminalProfileValidator.validate(
        TerminalProfileDocument(
            defaultProfileID: "lab",
            profiles: [
                TerminalProfileDefinition(
                    id: "lab",
                    title: "Lab User",
                    launch: .managedUser(unixUser: "lab"),
                    defaultWorkingDirectory: "/Users/lab",
                    presentation: nil
                ),
            ]
        )
    )
    try expect(
        missingManagedAccount.errors.contains(.missingManagedAccount("lab")),
        "managed_user profiles must require Managed User ownership evidence"
    )

    let mismatchedManagedAccount = TerminalProfileValidator.validate(
        TerminalProfileDocument(
            defaultProfileID: "lab",
            profiles: [
                TerminalProfileDefinition(
                    id: "lab",
                    title: "Lab User",
                    launch: .managedUser(unixUser: "lab"),
                    defaultWorkingDirectory: "/Users/lab",
                    presentation: nil,
                    managedTerminalAccountID: "other"
                ),
            ]
        )
    )
    try expect(
        mismatchedManagedAccount.errors.contains(
            .managedAccountMismatch(profileID: "lab", accountID: "other", unixUser: "lab")
        ),
        "managed_user profiles must launch the same Unix account that owns the profile"
    )

    let legacyMigrationPlan = ManagedTerminalAccountPlanner.plan(
        request: request,
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
            sudoers: .missing,
            ownership: alanManagedOwnership("lab"),
            terminalProfile: .existingManagedOutdated(profileID: "lab"),
            verification: .passed
        )
    )
    try expect(
        legacyMigrationPlan.steps.contains { $0.kind == .createOrUpdateTerminalProfile },
        "legacy Alan-managed sudo_user profiles must be planned for managed_user handoff"
    )

    let manualSudoDocument = TerminalProfileDocument(
        defaultProfileID: "ops",
        profiles: [
            TerminalProfileDefinition(
                id: "ops",
                title: "Operator",
                launch: .sudoUser(unixUser: "ops"),
                defaultWorkingDirectory: nil,
                presentation: nil
            ),
        ]
    )
    try expect(
        TerminalProfileValidator.validate(manualSudoDocument).isValid,
        "manual sudo_user profiles must remain operator managed and valid"
    )
}

private func testManagedUserRowsExposeStateAppropriateActions() throws {
    let readyPlan = ManagedTerminalAccountPlanner.plan(
        request: ManagedTerminalAccountRequest(
            accountName: "alan",
            guiUserName: "morris",
            fullName: "Alan Terminal"
        ),
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/alan", shell: "/bin/zsh", hidden: true),
            sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-alan"),
            ownership: alanManagedOwnership("alan"),
            terminalProfile: .existingManaged(profileID: "alan"),
            verification: .passed
        )
    )
    let repairPlan = ManagedTerminalAccountPlanner.plan(
        request: ManagedTerminalAccountRequest(
            accountName: "lab",
            guiUserName: "morris",
            fullName: "Lab User"
        ),
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
            sudoers: .missing,
            ownership: alanManagedOwnership("lab"),
            terminalProfile: .existingManaged(profileID: "lab"),
            verification: .passed
        )
    )
    let conflictPlan = ManagedTerminalAccountPlanner.plan(
        request: ManagedTerminalAccountRequest(
            accountName: "ops",
            guiUserName: "morris",
            fullName: "Ops User"
        ),
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/ops", shell: "/bin/zsh", hidden: true),
            sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-ops"),
            ownership: alanManagedOwnership("ops"),
            terminalProfile: .existingUnmanaged(profileID: "ops"),
            verification: .passed
        )
    )
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary(
            plans: [readyPlan, repairPlan, conflictPlan]
        )
    )
    let accountSection = try requireSection(.terminalAccounts, in: snapshot)
    let rowsByID = Dictionary(uniqueKeysWithValues: accountSection.rows.map { ($0.id, $0) })
    let visibleText = accountSection.visibleText.joined(separator: "\n")

    try expect(
        accountSection.rows.map(\.id).contains("terminalAccountProvision"),
        "Managed Users must keep the create action available when users already exist"
    )
    try expect(
        rowsByID["terminalAccountProvision"]?.actions.map(\.id) == [.create],
        "Managed Users create row must expose an explicit Create action"
    )
    try expect(
        rowsByID["terminalAccount.alan"]?.actions.map(\.id) == [.review, .verify, .remove],
        "Ready Managed Users must expose Review, Verify, and Remove actions"
    )
    try expect(
        rowsByID["terminalAccount.lab"]?.actions.map(\.id) == [.review, .repair],
        "Repairable Managed Users must expose Review and Repair actions"
    )
    try expect(
        rowsByID["terminalAccount.ops"]?.actions.map(\.id) == [.review],
        "Conflicting Managed Users must route through Review instead of direct repair"
    )
    try expect(
        !visibleText.contains("NOPASSWD") && !visibleText.contains("/usr/bin/dscl")
            && !visibleText.contains("do shell script"),
        "Managed Users rows must not expose raw sudoers or privileged script bodies"
    )
}

private func testManagedUserRowsExposeHelperBackedStates() throws {
    func request(_ accountName: String) -> ManagedTerminalAccountRequest {
        ManagedTerminalAccountRequest(
            accountName: accountName,
            guiUserName: "morris",
            fullName: "\(accountName) user"
        )
    }

    func plan(
        _ accountName: String,
        status: ManagedTerminalAccountPlanStatus,
        steps: [ManagedTerminalAccountPlanStep] = []
    ) -> ManagedTerminalAccountPlan {
        ManagedTerminalAccountPlan(
            request: request(accountName),
            status: status,
            steps: steps
        )
    }

    let helperVerifyStep = ManagedTerminalAccountPlanStep(
        kind: .helperStep(.verifyManagedUserPTY),
        summary: "Verify helper PTY",
        requiresPrivilege: true
    )
    let plans = [
        plan("ready", status: .alreadyReady),
        plan("repair", status: .repair, steps: [helperVerifyStep]),
        plan("manual", status: .accountNotAlanManaged),
        plan(
            "legacy",
            status: .legacySudoersPresent(path: "/etc/sudoers.d/alan-terminal-morris-to-legacy"),
            steps: [
                ManagedTerminalAccountPlanStep(
                    kind: .helperStep(.cleanupLegacySudoers),
                    summary: "Clean up legacy sudoers",
                    requiresPrivilege: true
                ),
            ]
        ),
        plan("ptyfail", status: .ptySpawnFailed, steps: [helperVerifyStep]),
        plan("delete", status: .requiresDestructiveConfirmation),
    ]
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary(plans: plans)
    )
    let accountSection = try requireSection(.terminalAccounts, in: snapshot)
    let rowsByID = Dictionary(uniqueKeysWithValues: accountSection.rows.map { ($0.id, $0) })

    try expect(
        rowsByID["terminalAccount.ready"]?.value == "Ready"
            && rowsByID["terminalAccount.ready"]?.actions.map(\.id) == [.review, .verify, .remove],
        "ready Managed User rows must remain selectable and removable"
    )
    try expect(
        rowsByID["terminalAccount.repair"]?.value == "Repairable"
            && rowsByID["terminalAccount.repair"]?.actions.map(\.id) == [.review, .repair],
        "repairable Managed User rows must expose repair"
    )
    try expect(
        rowsByID["terminalAccount.manual"]?.value == "Not managed"
            && rowsByID["terminalAccount.manual"]?.actions.map(\.id) == [.review],
        "ordinary macOS accounts must render as not Alan managed"
    )
    try expect(
        rowsByID["terminalAccount.legacy"]?.value == "Legacy"
            && rowsByID["terminalAccount.legacy"]?.actions.map(\.id) == [.review, .repair]
            && rowsByID["terminalAccount.legacy"]?.detail?.contains("legacy Alan sudoers state") == true,
        "legacy Alan sudoers state must render as a repairable cleanup row"
    )
    try expect(
        rowsByID["terminalAccount.ptyfail"]?.value == "PTY failed"
            && rowsByID["terminalAccount.ptyfail"]?.actions.map(\.id) == [.review, .repair],
        "helper PTY spawn failure must render as repairable helper state"
    )
    try expect(
        rowsByID["terminalAccount.delete"]?.value == "Confirm"
            && rowsByID["terminalAccount.delete"]?.actions.map(\.id) == [.review],
        "destructive Managed User rollback must render as confirmation-only"
    )
}

private func testManagedUserCatalogIncludesPersistedAndIncompleteUsers() throws {
    let terminalProfiles = TerminalProfileSettingsSummary(
        profiles: [.loginShellFallback],
        defaultProfileID: TerminalProfileDefinition.loginShellFallback.id,
        recoveryMessage: nil
    )
    let commandRunner = StubManagedTerminalAccountCommandRunner(
        responses: [
            "/usr/bin/dscl . -read /Users/liuyimeng1994 UniqueID PrimaryGroupID NFSHomeDirectory UserShell IsHidden AuthenticationAuthority":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: """
                    No such key: IsHidden
                    No such key: AuthenticationAuthority
                    NFSHomeDirectory: /Users/liuyimeng1994
                    PrimaryGroupID: 20
                    UniqueID: 504
                    UserShell: /bin/zsh
                    """,
                    standardError: ""
                ),
            "/usr/sbin/dseditgroup -o checkmember -m liuyimeng1994 admin":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: "yes liuyimeng1994 is a member of admin",
                    standardError: ""
                ),
            "/usr/bin/dscl . -read /Users/realmorrisliu UniqueID PrimaryGroupID NFSHomeDirectory UserShell IsHidden AuthenticationAuthority":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: """
                    No such key: IsHidden
                    No such key: AuthenticationAuthority
                    NFSHomeDirectory: /Users/realmorrisliu
                    PrimaryGroupID: 20
                    UniqueID: 505
                    UserShell: /bin/zsh
                    """,
                    standardError: ""
                ),
            "/usr/sbin/dseditgroup -o checkmember -m realmorrisliu admin":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: "yes realmorrisliu is a member of admin",
                    standardError: ""
                ),
            "/usr/bin/dscl . -read /Users/univer UniqueID PrimaryGroupID NFSHomeDirectory UserShell IsHidden AuthenticationAuthority":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: """
                    No such key: IsHidden
                    No such key: AuthenticationAuthority
                    No such key: UniqueID
                    NFSHomeDirectory: /Users/univer
                    PrimaryGroupID: 20
                    UserShell: /bin/zsh
                    """,
                    standardError: ""
                ),
            "/usr/sbin/dseditgroup -o checkmember -m univer admin":
                ManagedTerminalAccountCommandResult(
                    exitCode: 1,
                    standardOutput: "no univer is not a member of admin",
                    standardError: ""
                ),
        ]
    )
    let discoverer = ManagedTerminalAccountLocalStateDiscoverer(
        commandRunner: commandRunner,
        sudoersSyntaxChecker: StubSudoersSyntaxChecker(result: .passed)
    )
    let summary = ManagedTerminalAccountSettingsSummary.current(
        terminalProfiles: terminalProfiles,
        guiUserName: "morris",
        discoverer: discoverer,
        catalog: ManagedTerminalAccountCatalog(
            entries: [
                ManagedTerminalAccountCatalogEntry(accountName: "lab", displayLabel: "Lab User"),
            ]
        )
    )
    let usersByName = Dictionary(uniqueKeysWithValues: summary.users.map { ($0.unixUserName, $0) })

    try expect(
        summary.users.map(\.unixUserName) == ["lab"],
        "Managed Users must include persisted intents without importing ordinary local accounts"
    )
    try expect(
        usersByName["lab"]?.displayLabel == "Lab User"
            && usersByName["lab"]?.readinessState == .readyToApply,
        "persisted Managed User intents must remain manageable even before a Terminal Profile exists"
    )
    try expect(
        usersByName["liuyimeng1994"] == nil
            && usersByName["realmorrisliu"] == nil
            && usersByName["morris"] == nil
            && usersByName["root"] == nil
            && usersByName["univer"] == nil,
        "Managed Users must not import normal or reserved local accounts as managed identities"
    )
}

private func testManagedUserExistingOrdinaryAccountReportsNotAlanManaged() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "univer",
        guiUserName: "morris",
        fullName: "Univer"
    )
    let commandRunner = StubManagedTerminalAccountCommandRunner(
        responses: [
            "/usr/bin/dscl . -read /Users/univer UniqueID PrimaryGroupID NFSHomeDirectory UserShell IsHidden AuthenticationAuthority":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: """
                    No such key: AuthenticationAuthority
                    NFSHomeDirectory: /Users/univer
                    PrimaryGroupID: 20
                    UniqueID: 507
                    UserShell: /bin/zsh
                    """,
                    standardError: ""
                ),
            "/usr/sbin/dseditgroup -o checkmember -m univer admin":
                ManagedTerminalAccountCommandResult(
                    exitCode: 1,
                    standardOutput: "no univer is not a member of admin",
                    standardError: ""
                ),
        ]
    )
    let discoverer = ManagedTerminalAccountLocalStateDiscoverer(
        fileManager: SudoersFixtureFileManager(existingPaths: [request.homeDirectory]),
        commandRunner: commandRunner,
        sudoersSyntaxChecker: StubSudoersSyntaxChecker(result: .passed)
    )
    let summary = ManagedTerminalAccountSettingsSummary.current(
        terminalProfiles: TerminalProfileSettingsSummary(
            profiles: [.loginShellFallback],
            defaultProfileID: TerminalProfileDefinition.loginShellFallback.id,
            recoveryMessage: nil
        ),
        guiUserName: request.guiUserName,
        discoverer: discoverer,
        entryVerifier: StubTerminalEntryVerifier(result: .passed),
        catalog: ManagedTerminalAccountCatalog(
            entries: [
                ManagedTerminalAccountCatalogEntry(
                    accountName: request.accountName,
                    displayLabel: request.fullName ?? request.accountName
                ),
            ]
        )
    )
    guard let plan = summary.plans.first, let user = summary.users.first else {
        throw TestFailure.message("ordinary requested account must remain visible as not Alan managed")
    }
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        managedTerminalAccounts: summary
    )
    let accountSection = try requireSection(.terminalAccounts, in: snapshot)
    let ordinaryRow = accountSection.rows.first { $0.id == "terminalAccount.univer" }

    try expect(
        plan.status == .accountNotAlanManaged,
        "ordinary account plan must not be repairable, got \(plan.status)"
    )
    try expect(plan.steps.isEmpty, "ordinary accounts must not produce privileged repair steps")
    try expect(
        user.readinessState == .accountNotAlanManaged,
        "ordinary account summaries must report accountNotAlanManaged"
    )
    try expect(
        ordinaryRow?.value == "Not managed"
            && ordinaryRow?.actions.map(\.id) == [.review],
        "ordinary account rows must be review-only and not expose repair/remove"
    )
    try expect(
        ordinaryRow?.detail?.contains("existing local account") == true,
        "ordinary account rows must explain that Alan will not take over the user"
    )
}

private func testManagedUserSummaryUsesHelperDiagnosisStates() throws {
    let profiles = TerminalProfileSettingsSummary(
        profiles: [
            .loginShellFallback,
            TerminalProfileDefinition(
                id: "ready",
                title: "Ready User",
                launch: .managedUser(unixUser: "ready"),
                defaultWorkingDirectory: "/Users/ready",
                presentation: nil,
                managedTerminalAccountID: "ready"
            ),
        ],
        defaultProfileID: "ready",
        recoveryMessage: nil
    )
    let helper = AlanPrivilegedHelperFakeClient(
        channel: .dev,
        diagnosesByAccount: [
            "ready": helperDiagnosis(
                accountName: "ready",
                readiness: .ready,
                ownership: .alanManaged,
                terminalProfileID: "ready",
                ptySmokeVerified: true
            ),
            "manual": helperDiagnosis(
                accountName: "manual",
                readiness: .accountNotAlanManaged,
                ownership: .notAlanManaged
            ),
            "legacy": helperDiagnosis(
                accountName: "legacy",
                readiness: .legacySudoersPresent,
                ownership: .alanManaged,
                legacySudoersPath: "/etc/sudoers.d/alan-terminal-morris-to-legacy"
            ),
            "foreign": helperDiagnosis(
                accountName: "foreign",
                readiness: .legacySudoersPresent,
                ownership: .alanManaged,
                legacySudoersPath: "/etc/sudoers.d/operator-owned-foreign"
            ),
            "ptyfail": helperDiagnosis(
                accountName: "ptyfail",
                readiness: .ptySpawnFailed,
                ownership: .alanManaged,
                ptySmokeVerified: false
            ),
        ]
    )
    let summary = ManagedTerminalAccountSettingsSummary.current(
        terminalProfiles: profiles,
        guiUserName: "morris",
        helperClient: helper,
        catalog: ManagedTerminalAccountCatalog(
            entries: ["ready", "manual", "legacy", "foreign", "ptyfail"].map {
                ManagedTerminalAccountCatalogEntry(accountName: $0, displayLabel: $0)
            }
        )
    )
    let plans = Dictionary(uniqueKeysWithValues: summary.plans.map { ($0.request.accountName, $0) })
    let users = Dictionary(uniqueKeysWithValues: summary.users.map { ($0.unixUserName, $0) })

    try expect(plans["ready"]?.status == .alreadyReady, "helper-ready diagnosis must produce ready state")
    try expect(
        plans["manual"]?.status == .accountNotAlanManaged
            && plans["manual"]?.steps.isEmpty == true,
        "helper accountNotAlanManaged diagnosis must not produce repair steps"
    )
    try expect(
        plans["legacy"]?.status
            == .legacySudoersPresent(path: "/etc/sudoers.d/alan-terminal-morris-to-legacy"),
        "helper legacy sudoers diagnosis must preserve sanitized cleanup state"
    )
    try expect(
        plans["legacy"]?.steps.contains { $0.kind == .helperStep(.cleanupLegacySudoers) } == true,
        "legacy helper diagnosis must plan helper-owned sudoers cleanup"
    )
    try expect(
        plans["foreign"]?.status == .sudoersConflict(path: "/etc/sudoers.d/operator-owned-foreign")
            && plans["foreign"]?.steps.contains { $0.kind == .helperStep(.cleanupLegacySudoers) } != true,
        "non-Alan sudoers paths must be preserved and reported as conflict"
    )
    try expect(
        plans["ptyfail"]?.status == .ptySpawnFailed
            && plans["ptyfail"]?.steps.contains { $0.kind == .helperStep(.verifyManagedUserPTY) } == true,
        "helper PTY failure diagnosis must plan helper-owned PTY verification"
    )
    for plan in summary.plans {
        try expect(
            !plan.steps.contains {
                $0.kind == .writeSudoersDropIn
                    || $0.kind == .validateSudoers
                    || $0.kind == .verifyTerminalEntry
            },
            "helper-backed diagnosis must not schedule sudoers or sudo readiness fallback"
        )
    }
    try expect(
        users["legacy"]?.readinessState == .legacySudoersPresent
            && users["ptyfail"]?.readinessState == .ptySpawnFailed,
        "Managed User summaries must expose helper diagnosis states"
    )

    let unavailableSummary = ManagedTerminalAccountSettingsSummary.current(
        terminalProfiles: profiles,
        guiUserName: "morris",
        helperClient: AlanPrivilegedHelperFakeClient(channel: .dev, statusState: .invalidSignature),
        catalog: ManagedTerminalAccountCatalog(
            entries: [
                ManagedTerminalAccountCatalogEntry(accountName: "ready", displayLabel: "Ready User"),
            ]
        )
    )
    try expect(
        unavailableSummary.plans.first?.status == .helperUnavailable,
        "unhealthy helper status must short-circuit helper diagnosis as unavailable"
    )
}

private func testManagedUserReviewPreviewUsesHelperOperationRows() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "legacy",
        guiUserName: "morris",
        fullName: "Legacy User"
    )
    let plan = ManagedTerminalAccountPlanner.plan(
        request: request,
        diagnosis: helperDiagnosis(
            accountName: "legacy",
            readiness: .legacySudoersPresent,
            ownership: .alanManaged,
            legacySudoersPath: "/etc/sudoers.d/alan-terminal-morris-to-legacy",
            ptySmokeVerified: false
        )
    )
    let preview = ManagedTerminalUserCreationPreview(request: request, plan: plan)
    let rows = preview.visiblePlanRows.joined(separator: "\n")

    try expect(
        rows.contains("Privileged helper managed"),
        "Managed User review sheets must identify helper-backed privilege ownership"
    )
    try expect(
        rows.contains("Clean up verified legacy Alan sudoers"),
        "Managed User review sheets must show helper-owned legacy cleanup"
    )
    try expect(
        rows.contains("Verify helper-managed PTY startup"),
        "Managed User review sheets must show helper PTY verification"
    )
    try expect(
        !rows.contains("Write Alan-owned sudoers drop-in")
            && !rows.contains("Validate sudoers syntax")
            && !rows.contains("sudo -n -iu"),
        "Managed User review sheets must not describe sudoers setup or sudo readiness fallback"
    )
}

private func testManagedUserCreationPreviewDerivesDefaultsAndRejectsConflicts() throws {
    let draft = ManagedTerminalUserCreationDraft(
        unixUserName: "lab",
        displayLabel: "Lab User",
        guiUserName: "morris"
    )
    let result = ManagedTerminalUserCreationPreviewBuilder.make(
        draft: draft,
        existingUsers: [],
        terminalProfiles: testTerminalProfiles(),
        diagnosis: helperDiagnosis(
            accountName: "lab",
            readiness: .accountMissing,
            ownership: .missing,
            homeDirectoryExists: false,
            hiddenFromLoginWindow: false
        )
    )
    guard let preview = result.preview else {
        throw TestFailure.message("Managed User creation preview must be available for valid input")
    }

    try expect(result.isValid, "valid Managed User draft must produce a valid preview")
    try expect(preview.request.accountName == "lab", "preview must derive the Unix account name")
    try expect(preview.request.fullName == "Lab User", "preview must preserve the display label")
    try expect(preview.request.guiUserName == "morris", "preview must preserve the GUI user")
    try expect(preview.request.homeDirectory == "/Users/lab", "preview must derive the home directory")
    try expect(preview.request.shell == "/bin/zsh", "preview must use the Login shell-compatible zsh")
    try expect(preview.request.hideFromLoginWindow, "preview must keep the Managed User hidden from login")
    try expect(
        !preview.request.bindCurrentSpaceAfterSuccess,
        "preview must not bind the current Space as a side effect"
    )
    try expect(
        preview.plan.steps.map(\.kind) == [
            .helperStep(.createStandardAccount),
            .helperStep(.hideAccount),
            .helperStep(.writeOwnershipMarker),
            .helperStep(.verifyAccount),
            .helperStep(.verifyManagedUserPTY),
            .createOrUpdateTerminalProfile,
        ],
        "creation preview must show the helper-backed plan in execution order"
    )

    let previewText = preview.visiblePlanRows.joined(separator: "\n")
    for expected in [
        "Account lab",
        "Home /Users/lab",
        "Shell /bin/zsh",
        "Hidden from login window",
        "Privileged helper managed",
        "Create standard local terminal account",
        "Hide terminal account from login window lists",
        "Write Alan-managed ownership marker",
        "Verify helper-managed account state",
        "Verify helper-managed PTY startup",
        "Terminal Profile lab",
    ] {
        try expect(previewText.contains(expected), "preview must include \(expected)")
    }
    try expect(
        !previewText.contains("NOPASSWD") && !previewText.contains("password")
            && !previewText.contains("/usr/bin/dscl"),
        "creation preview must stay compact and avoid raw scripts or secrets"
    )

    let duplicateUser = ManagedTerminalUserSummary(plan: preview.plan)
    let duplicateResult = ManagedTerminalUserCreationPreviewBuilder.make(
        draft: draft,
        existingUsers: [duplicateUser],
        terminalProfiles: testTerminalProfiles(),
        diagnosis: helperDiagnosis(
            accountName: "lab",
            readiness: .accountMissing,
            ownership: .missing,
            homeDirectoryExists: false,
            hiddenFromLoginWindow: false
        )
    )
    try expect(
        duplicateResult.errors.contains(.duplicateUnixUser("lab")),
        "creation preview must reject duplicate Managed User Unix names"
    )

    let baseProfiles = testTerminalProfiles()
    let conflictingProfiles = TerminalProfileSettingsSummary(
        profiles: baseProfiles.profiles + [
            TerminalProfileDefinition(
                id: "lab",
                title: "Lab",
                launch: .sudoUser(unixUser: "lab"),
                defaultWorkingDirectory: "/Users/lab",
                presentation: nil
            )
        ],
        defaultProfileID: baseProfiles.defaultProfileID,
        recoveryMessage: baseProfiles.recoveryMessage
    )
    let conflictResult = ManagedTerminalUserCreationPreviewBuilder.make(
        draft: draft,
        existingUsers: [],
        terminalProfiles: conflictingProfiles,
        diagnosis: helperDiagnosis(
            accountName: "lab",
            readiness: .accountMissing,
            ownership: .missing,
            homeDirectoryExists: false,
            hiddenFromLoginWindow: false
        )
    )
    try expect(
        conflictResult.errors.contains(.terminalProfileConflict("lab")),
        "creation preview must reject conflicting unmanaged Terminal Profiles"
    )

    let incompleteResult = ManagedTerminalUserCreationPreviewBuilder.make(
        draft: ManagedTerminalUserCreationDraft(
            unixUserName: "univer",
            displayLabel: "Univer",
            guiUserName: "morris"
        ),
        existingUsers: [],
        terminalProfiles: baseProfiles,
        diagnosis: helperDiagnosis(
            accountName: "univer",
            readiness: .repairable,
            ownership: .alanManaged,
            ptySmokeVerified: false
        )
    )
    try expect(
        incompleteResult.isValid,
        "creation preview must allow repairing incomplete local account records"
    )
    try expect(
        incompleteResult.preview?.plan.steps.contains {
            $0.kind == .helperStep(.verifyManagedUserPTY)
        } == true,
        "repairable account creation preview must route through helper repair operations"
    )
}

private func testManagedUserSummaryRunsReadinessVerificationBeforePlanning() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "lab",
        guiUserName: "morris",
        fullName: "Lab User"
    )
    let rule = ManagedTerminalAccountSudoersRule(request: request)
    let terminalProfiles = TerminalProfileSettingsSummary(
        profiles: [
            TerminalProfileDefinition(
                id: request.terminalProfileID,
                title: "Lab User",
                launch: .managedUser(unixUser: request.accountName),
                defaultWorkingDirectory: request.homeDirectory,
                presentation: nil,
                managedTerminalAccountID: request.accountName
            ),
        ],
        defaultProfileID: request.terminalProfileID,
        recoveryMessage: nil
    )
    let commandRunner = StubManagedTerminalAccountCommandRunner(
        responses: [
            "/usr/bin/dscl . -read /Users/lab UniqueID PrimaryGroupID NFSHomeDirectory UserShell IsHidden AuthenticationAuthority":
                ManagedTerminalAccountCommandResult(
                    exitCode: 0,
                    standardOutput: """
                    dsAttrTypeNative:IsHidden: 1
                    NFSHomeDirectory: /Users/lab
                    PrimaryGroupID: 20
                    UniqueID: 507
                    UserShell: /bin/zsh
                    """,
                    standardError: "No such key: AuthenticationAuthority"
                ),
            "/usr/sbin/dseditgroup -o checkmember -m lab admin":
                ManagedTerminalAccountCommandResult(
                    exitCode: 1,
                    standardOutput: "no lab is not a member of admin",
                    standardError: ""
                ),
        ]
    )
    let discoverer = ManagedTerminalAccountLocalStateDiscoverer(
        fileManager: SudoersFixtureFileManager(
            files: [rule.filePath: rule.contents],
            existingPaths: [request.homeDirectory]
        ),
        commandRunner: commandRunner,
        sudoersSyntaxChecker: StubSudoersSyntaxChecker(result: .passed)
    )

    let summary = ManagedTerminalAccountSettingsSummary.current(
        terminalProfiles: terminalProfiles,
        guiUserName: request.guiUserName,
        discoverer: discoverer,
        entryVerifier: StubTerminalEntryVerifier(result: .passed),
        catalog: ManagedTerminalAccountCatalog(
            entries: [
                ManagedTerminalAccountCatalogEntry(
                    accountName: request.accountName,
                    displayLabel: request.fullName ?? request.accountName
                ),
            ]
        )
    )

    try expect(
        summary.users.first?.readinessState == .ready,
        "Managed User summaries must run terminal-entry verification before planning readiness"
    )
    try expect(
        summary.plans.first?.steps.contains {
            $0.kind == ManagedTerminalAccountPlanStepKind.verifyTerminalEntry
        } != true,
        "verified Managed User summaries must not keep scheduling no-op repair verification"
    )
}

private func testManagedUserApplyUsesPrivilegedExecutorAndRefreshesStatus() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "lab",
        guiUserName: "morris",
        fullName: "Lab User"
    )
    let plan = ManagedTerminalAccountPlanner.plan(
        request: request,
        state: ManagedTerminalAccountState(
            account: .missing,
            sudoers: .missing,
            terminalProfile: .missing,
            verification: .notRun
        )
    )
    let refreshedPlan = ManagedTerminalAccountPlanner.plan(
        request: request,
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
            sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-lab"),
            ownership: alanManagedOwnership("lab"),
            terminalProfile: .existingManaged(profileID: "lab"),
            verification: .passed
        )
    )
    let executor = ManagedTerminalAccountFakeExecutor()
    var refreshCount = 0

    let result = ManagedTerminalUserProvisioningFlow.applyApproved(
        plan: plan,
        executor: executor
    ) {
        refreshCount += 1
        return ManagedTerminalAccountSettingsSummary(plans: [refreshedPlan])
    }

    try expect(
        result.applyResult.completedSteps == plan.steps.map(\.kind),
        "apply flow must execute the approved plan through the privileged executor"
    )
    try expect(refreshCount == 1, "apply flow must refresh Managed User status after apply")
    try expect(
        result.refreshedSummary.users.first?.readinessState == .ready,
        "apply flow must return the refreshed readiness summary"
    )
    try expect(
        !result.applyResult.visibleDiagnostics.joined(separator: "\n").contains("NOPASSWD"),
        "apply diagnostics must stay redacted"
    )
}

private func testManagedUserApplyUsesHelperDeclarativePlanAndRejectsLegacySteps() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "helper",
        guiUserName: "morris",
        fullName: "Helper User"
    )
    let helperPlan = ManagedTerminalAccountPlanner.plan(
        request: request,
        diagnosis: helperDiagnosis(
            accountName: request.accountName,
            readiness: .accountMissing,
            ownership: .missing
        )
    )
    let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
    let storeDirectory = try makeTemporaryDirectory()
    defer { try? FileManager.default.removeItem(at: storeDirectory) }
    let storeURL = storeDirectory.appendingPathComponent("terminal-profiles.json", isDirectory: false)
    let executor = ManagedTerminalAccountHelperExecutor(
        channel: .dev,
        helperClient: helper,
        localEffectExecutor: ManagedTerminalAccountTerminalProfileEffectExecutor(
            store: TerminalProfileStore(storeURL: storeURL)
        )
    )
    let result = executor.apply(helperPlan)
    let savedProfiles = TerminalProfileStore(storeURL: storeURL).load().document

    try expect(result.failedStep == nil, "helper-authored Managed User plan must apply")
    try expect(helper.appliedPlans.count == 1, "helper executor must send one typed helper plan")
    try expect(
        helper.appliedPlans.first?.steps.map(\.kind).contains(.createStandardAccount) == true
            && helper.appliedPlans.first?.steps.map(\.kind).contains(.writeOwnershipMarker) == true
            && helper.appliedPlans.first?.steps.map(\.kind).contains(.verifyManagedUserPTY) == true,
        "helper executor must preserve typed helper operations"
    )
    try expect(
        helper.appliedPlans.first?.steps.map(\.kind).contains(.cleanupLegacySudoers) == false,
        "helper apply must not invent legacy cleanup without helper diagnosis"
    )
    try expect(
        result.completedSteps.contains(.createOrUpdateTerminalProfile),
        "helper-backed apply must still perform local Terminal Profile handoff"
    )
    try expect(
        savedProfiles.profile(id: request.terminalProfileID)?.launch == .managedUser(unixUser: request.accountName),
        "helper-backed apply must create managed_user Terminal Profiles"
    )

    let legacySudoersPlan = ManagedTerminalAccountPlanner.plan(
        request: request,
        state: ManagedTerminalAccountState(
            account: .missing,
            sudoers: .missing,
            terminalProfile: .missing,
            verification: .notRun
        )
    )
    let rejected = executor.apply(legacySudoersPlan)
    try expect(
        rejected.failedStep == .createStandardAccount,
        "helper executor must reject legacy privileged account steps before helper apply"
    )
    try expect(
        helper.appliedPlans.count == 1,
        "helper executor must not send rejected legacy plans to the helper"
    )
    try expect(
        !rejected.visibleDiagnostics.joined(separator: "\n").contains("NOPASSWD")
            && !rejected.visibleDiagnostics.joined(separator: "\n").contains("do shell script")
            && !rejected.visibleDiagnostics.joined(separator: "\n").contains("/etc/sudoers"),
        "helper executor rejection diagnostics must stay sanitized"
    )
}

private func testFakeHelperCoversManagedUserRemovalPtyAndDenialStates() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "helper",
        guiUserName: "morris",
        fullName: "Helper User"
    )
    let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
    let removal = helper.removeManagedUserIntegration(request)
    try expect(
        removal.failedStep == nil
            && removal.completedSteps.contains(.removeManagedTerminalProfile),
        "fake helper must cover Managed User integration removal"
    )

    let ptyStart = helper.startManagedUserPTY(
        AlanManagedUserPTYStartRequest(
            operationID: "op-start",
            channelID: "dev",
            accountName: request.accountName,
            homeDirectory: request.homeDirectory,
            shell: request.shell,
            contentID: "content_fake_helper",
            columns: 80,
            rows: 24
        )
    )
    let session: AlanManagedUserPTYSession
    switch ptyStart {
    case .success(let started):
        session = started
    case .failure(let diagnostic):
        throw TestFailure.message("fake helper PTY start must succeed: \(diagnostic.sanitizedMessage)")
    }

    let terminateDiagnostic = helper.terminatePTY(sessionID: session.sessionID)
    try expect(
        helper.terminatedPTYSessionIDs == [session.sessionID]
            && terminateDiagnostic.operation == .terminatePTY
            && helper.observeManagedUserPTYExit(sessionID: session.sessionID)?.final == true,
        "fake helper must record PTY termination and final exit observation"
    )

    let denied = AlanPrivilegedHelperFakeClient(channel: .dev)
    denied.deniedOperation = .startManagedUserPTY
    let deniedStart = denied.startManagedUserPTY(
        AlanManagedUserPTYStartRequest(
            operationID: "op-denied",
            channelID: "dev",
            accountName: request.accountName,
            homeDirectory: request.homeDirectory,
            shell: request.shell,
            contentID: "content_denied",
            columns: 80,
            rows: 24
        )
    )
    if case .failure(let diagnostic) = deniedStart {
        try expect(
            diagnostic.code == .ptySpawnFailed
                && !diagnostic.sanitizedMessage.contains("sudo -n -iu")
                && !diagnostic.sanitizedMessage.contains("/etc/sudoers"),
            "fake helper PTY denial must stay typed and sanitized"
        )
    } else {
        throw TestFailure.message("fake helper PTY denial must fail")
    }
}

private func testManagedUserRollbackRequiresAlanOwnershipForDestructiveDeletion() throws {
    let request = ManagedTerminalAccountRequest(
        accountName: "lab",
        guiUserName: "morris",
        fullName: "Lab User"
    )
    let alanManagedState = ManagedTerminalAccountState(
        account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
        sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-lab"),
        ownership: alanManagedOwnership("lab"),
        terminalProfile: .existingManaged(profileID: "lab"),
        verification: .passed
    )
    let integrationOnly = ManagedTerminalAccountPlanner.rollbackPlan(
        request: request,
        state: alanManagedState,
        scope: .alanIntegrationOnly
    )
    let unconfirmed = ManagedTerminalAccountPlanner.rollbackPlan(
        request: request,
        state: alanManagedState,
        scope: .deleteAccountAndHome(confirmation: nil)
    )
    let confirmed = ManagedTerminalAccountPlanner.rollbackPlan(
        request: request,
        state: alanManagedState,
        scope: .deleteAccountAndHome(confirmation: "lab")
    )

    try expect(
        integrationOnly.steps.contains { $0.kind == .deleteAccount || $0.kind == .deleteHomeDirectory } == false,
        "integration-only rollback must not delete accounts or homes"
    )
    try expect(
        unconfirmed.status == .requiresDestructiveConfirmation
            && unconfirmed.steps.contains { $0.kind == .deleteAccount || $0.kind == .deleteHomeDirectory } == false,
        "destructive rollback must require a separate account-name confirmation"
    )
    try expect(
        confirmed.steps.contains { $0.kind == .deleteAccount }
            && confirmed.steps.contains { $0.kind == .deleteHomeDirectory },
        "confirmed destructive rollback may delete only Alan-managed account and canonical home"
    )

    let ordinaryState = ManagedTerminalAccountState(
        account: .standard(homeDirectory: "/Users/manual", shell: "/bin/zsh", hidden: false),
        sudoers: .missing,
        terminalProfile: .missing,
        verification: .notRun
    )
    let ordinaryRequest = ManagedTerminalAccountRequest(
        accountName: "manual",
        guiUserName: "morris",
        fullName: "Manual"
    )
    let ordinaryDelete = ManagedTerminalAccountPlanner.rollbackPlan(
        request: ordinaryRequest,
        state: ordinaryState,
        scope: .deleteAccountAndHome(confirmation: "manual")
    )
    try expect(
        ordinaryDelete.status == .accountNotAlanManaged
            && ordinaryDelete.steps.contains { $0.kind == .deleteAccount || $0.kind == .deleteHomeDirectory } == false,
        "ordinary local accounts must not be deleted through Managed User rollback"
    )
}

private func testManagedProfileReadinessFiltersSpaceIdentityChoices() throws {
    let readyPlan = ManagedTerminalAccountPlanner.plan(
        request: ManagedTerminalAccountRequest(
            accountName: "alan",
            guiUserName: "morris",
            fullName: "Alan Terminal"
        ),
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/alan", shell: "/bin/zsh", hidden: true),
            sudoers: .alanOwnedValid(path: "/etc/sudoers.d/alan-terminal-morris-to-alan"),
            ownership: alanManagedOwnership("alan"),
            terminalProfile: .existingManaged(profileID: "alan"),
            verification: .passed
        )
    )
    let repairPlan = ManagedTerminalAccountPlanner.plan(
        request: ManagedTerminalAccountRequest(
            accountName: "lab",
            guiUserName: "morris",
            fullName: "Lab User"
        ),
        state: ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/lab", shell: "/bin/zsh", hidden: true),
            sudoers: .missing,
            ownership: alanManagedOwnership("lab"),
            terminalProfile: .existingManaged(profileID: "lab"),
            verification: .passed
        )
    )
    let helperUnavailablePlan = ManagedTerminalAccountPlan(
        request: ManagedTerminalAccountRequest(
            accountName: "broken",
            guiUserName: "morris",
            fullName: "Broken User"
        ),
        status: .helperUnavailable,
        steps: []
    )
    let profiles = TerminalProfileSettingsSummary(
        profiles: [
            TerminalProfileDefinition.loginShellFallback,
            TerminalProfileDefinition(
                id: "alan",
                title: "Alan Terminal",
                launch: .managedUser(unixUser: "alan"),
                defaultWorkingDirectory: "/Users/alan",
                presentation: nil,
                managedTerminalAccountID: "alan"
            ),
            TerminalProfileDefinition(
                id: "lab",
                title: "Lab User",
                launch: .managedUser(unixUser: "lab"),
                defaultWorkingDirectory: "/Users/lab",
                presentation: nil,
                managedTerminalAccountID: "lab"
            ),
            TerminalProfileDefinition(
                id: "broken",
                title: "Broken User",
                launch: .managedUser(unixUser: "broken"),
                defaultWorkingDirectory: "/Users/broken",
                presentation: nil,
                managedTerminalAccountID: "broken"
            ),
            TerminalProfileDefinition(
                id: "custom",
                title: "Bootstrap",
                launch: .customCommand("echo redacted"),
                defaultWorkingDirectory: nil,
                presentation: nil
            ),
        ],
        defaultProfileID: "alan",
        recoveryMessage: nil
    )
    let managedAccounts = ManagedTerminalAccountSettingsSummary(
        plans: [readyPlan, repairPlan, helperUnavailablePlan]
    )
    let selectableProfileIDs = TerminalProfileSpaceIdentityFilter.selectableProfiles(
        terminalProfiles: profiles,
        managedTerminalAccounts: managedAccounts
    ).map(\.id)

    try expect(
        selectableProfileIDs == ["alan", "custom"],
        "Space identity choices must include unmanaged profiles and ready Managed Users only"
    )
    try expect(
        !selectableProfileIDs.contains(TerminalProfileDefinition.loginShellFallback.id),
        "Space identity choices must preserve Login shell as the nil-profile fallback"
    )
    try expect(
        TerminalProfileSpaceIdentityFilter.repairGuidance(
            profileID: "lab",
            terminalProfiles: profiles,
            managedTerminalAccounts: managedAccounts
        )?.contains("Repair") == true,
        "not-ready managed profiles must expose repair guidance"
    )
    try expect(
        TerminalProfileSpaceIdentityFilter.repairGuidance(
            profileID: "broken",
            terminalProfiles: profiles,
            managedTerminalAccounts: managedAccounts
        )?.contains("Privileged helper") == true,
        "helper-unready managed profiles must expose helper repair guidance"
    )
}

private func testPerformanceDiagnosticsRowsAreCompactAndLocal() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        diagnostics: ShellSettingsDiagnosticsSummary(
            isEnabled: false,
            retainedEventCount: 24,
            stutterMarkerCount: 2,
            lastExportURL: nil
        )
    )
    let local = try requireSection(.local, in: snapshot)
    let diagnosticsRows = local.rows.filter { $0.id.hasPrefix("performanceDiagnostics") }
    let visibleText = diagnosticsRows.flatMap(\.visibleText).joined(separator: "\n")

    try expect(
        diagnosticsRows.map(\.id) == ["performanceDiagnostics", "performanceDiagnosticsExport"],
        "settings must expose only a diagnostics toggle and recent export action"
    )
    try expect(
        diagnosticsRows.first?.mutability == .editable,
        "performance diagnostics toggle must be directly editable"
    )
    try expect(
        diagnosticsRows.last?.mutability == .actionOnly,
        "performance diagnostics export must be an action row"
    )
    try expect(
        visibleText.contains("Local performance trace"),
        "diagnostics copy must explain that capture is local"
    )
    try expect(
        visibleText.contains("Terminal content is not recorded"),
        "diagnostics copy must state that terminal content is not recorded"
    )
    try expect(
        !visibleText.localizedCaseInsensitiveContains("dashboard")
            && !visibleText.localizedCaseInsensitiveContains("inspector"),
        "diagnostics settings copy must not introduce dashboard or inspector framing"
    )
}

private func testNavigationGroupsMapTaskOrientedRows() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let groups = snapshot.navigationGroups

    try expect(
        groups.map(\.id) == [.general, .terminal, .agent, .system],
        "settings navigation groups must use General, Terminal, Agent, and System"
    )

    let general = try requireNavigationGroup(.general, in: snapshot)
    try expect(
        general.sections.map(\.id) == [.interface],
        "General must contain only Interface preferences"
    )
    try expect(
        general.rows.map(\.id) == ["appearance", "sidebar", "inactiveSplitDimming"],
        "General must expose the existing direct interface controls"
    )

    let terminal = try requireNavigationGroup(.terminal, in: snapshot)
    try expect(
        terminal.sections.map(\.id) == [.profiles, .localIdentity],
        "Terminal must group terminal profiles and local terminal identity"
    )
    let terminalRowsByID = Dictionary(uniqueKeysWithValues: terminal.rows.map { ($0.id, $0) })
    try expect(
        terminal.sections.first(where: { $0.id == .localIdentity })?.title == "Managed Users",
        "Terminal local identity section must be labeled Managed Users"
    )
    try expect(
        terminalRowsByID["terminalProfilesDefault"]?.title == "Default profile"
            && terminalRowsByID["terminalProfilesDefault"]?.value == "Alan",
        "Terminal settings must include the shell-core default profile row"
    )
    try expect(
        terminalRowsByID["terminalProfilesCreate"]?.title == "New profile"
            && terminalRowsByID["terminalProfilesCreate"]?.value == "Create…"
            && terminalRowsByID["terminalProfilesCreate"]?.detail == "Create a local startup profile.",
        "Terminal create row must use native ellipsis action copy with concise detail"
    )
    try expect(
        terminalRowsByID["terminalProfile.login_shell"]?.detail == nil,
        "Terminal login shell row must not repeat its title as secondary copy"
    )
}

private func testNavigationGroupsKeepTerminalIdentityOutOfAgent() throws {
    let accountPlan = ManagedTerminalAccountPlanner.plan(
        request: ManagedTerminalAccountRequest(accountName: "alan", guiUserName: "morris"),
        state: ManagedTerminalAccountState(
            account: .missing,
            sudoers: .missing,
            terminalProfile: .missing,
            verification: .notRun
        )
    )
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary(plans: [accountPlan])
    )
    let terminal = try requireNavigationGroup(.terminal, in: snapshot)
    let agent = try requireNavigationGroup(.agent, in: snapshot)

    try expect(
        terminal.rows.map(\.id).contains("terminalProfilesDefault"),
        "Terminal must contain the Default Terminal Profile row"
    )
    try expect(
        terminal.rows.contains { $0.id.hasPrefix("terminalAccount.") },
        "Terminal must contain Managed Terminal Account rows"
    )
    try expect(
        terminal.rows.map(\.id).contains("terminalProfilesSudoGuidance"),
        "Terminal must contain sudo behavior guidance"
    )
    try expect(
        !agent.rows.contains { $0.id.hasPrefix("terminalProfile") || $0.id.hasPrefix("terminalAccount") },
        "Agent must not contain local terminal identity rows"
    )
}

private func testNavigationGroupsPlaceAgentAndSystemRows() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        diagnostics: ShellSettingsDiagnosticsSummary(
            isEnabled: false,
            retainedEventCount: 24,
            stutterMarkerCount: 2,
            lastExportURL: nil
        )
    )
    let agent = try requireNavigationGroup(.agent, in: snapshot)
    let system = try requireNavigationGroup(.system, in: snapshot)

    try expect(
        agent.rows.map(\.id).contains("agentSelector"),
        "Agent must expose the currently configurable Alan agent"
    )
    try expect(
        agent.rows.map(\.id).contains("accountsUnavailable"),
        "Agent must contain compact provider connection unavailable state"
    )
    try expect(
        ["governance", "reasoningEffort", "streamingMode", "recoveryMode"]
            .allSatisfy(agent.rows.map(\.id).contains),
        "Agent must contain runtime default rows"
    )
    try expect(
        agent.rows.map(\.id).contains("capabilitiesUnavailable"),
        "Agent must contain compact skill catalog unavailable state"
    )
    try expect(
        agent.rows.contains { $0.id == "publicSkills" && $0.title == "Skill packages" },
        "Agent must contain the renamed skill package path row"
    )
    try expect(
        agent.sections.map(\.id) == [.agent, .runtimeDefaults, .skills, .entryPoints],
        "Agent must merge connection and skill source details into task-oriented sections"
    )
    try expect(
        agent.rows.map(\.id).contains("cliTool"),
        "Agent must contain the command line tool entry point"
    )

    let connectedSnapshot = ShellSettingsSurfaceSnapshot.make(
        remote: ShellSettingsRemoteSnapshot(
            accounts: ShellSettingsAccountsSummary(
                current: ShellSettingsConnectionSelection(
                    defaultProfile: "chatgpt-main",
                    effectiveProfile: "chatgpt-main",
                    effectiveSource: "global"
                ),
                profiles: [
                    ShellSettingsConnectionProfile(
                        profileID: "chatgpt-main",
                        label: "ChatGPT",
                        provider: "chatgpt",
                        credentialStatus: "available",
                        settings: [:],
                        isDefault: true
                    )
                ],
                providers: [],
                unavailableReason: nil
            ),
            capabilities: ShellSettingsCapabilitiesSummary(
                skills: [
                    ShellSettingsSkillSummary(
                        id: "memory",
                        name: "Memory",
                        enabled: true,
                        allowImplicitInvocation: false,
                        available: true
                    ),
                    ShellSettingsSkillSummary(
                        id: "plan",
                        name: "Plan",
                        enabled: false,
                        allowImplicitInvocation: false,
                        available: true
                    ),
                ],
                unavailableReason: nil
            )
        ),
        local: devLocalSummary()
    )
    let connectedAgent = try requireNavigationGroup(.agent, in: connectedSnapshot)
    let connectedRowIDs = connectedAgent.rows.map { $0.id }
    let connectionRow = connectedAgent.rows.first { $0.id == "selectedProfile" }
    let skillCatalogRow = connectedAgent.rows.first { $0.id == "capabilitiesAvailable" }
    try expect(
        connectionRow?.title == "Connection profile" && connectionRow?.detail == nil,
        "Agent connection row must stay compact when profile metadata is available"
    )
    try expect(
        skillCatalogRow?.title == "Skill catalog" && skillCatalogRow?.detail == nil,
        "Agent skill catalog row must stay compact when capability metadata is available"
    )
    try expect(
        !connectedRowIDs.contains("enabledSkills")
            && !connectedRowIDs.contains("implicitInvocation")
            && !connectedRowIDs.contains("unavailableSkills"),
        "Agent must not expand skill catalog into debug-count rows"
    )

    try expect(
        ["appIdentity", "installChannel", "updates", "daemonEndpoint", "dataRoot",
         "applicationSupport", "shellControl"].allSatisfy(system.rows.map(\.id).contains),
        "System must contain app, runtime, storage, and shell control rows"
    )
    let rowsByID = Dictionary(uniqueKeysWithValues: system.rows.map { ($0.id, $0) })
    try expect(
        rowsByID["updates"]?.detail == nil,
        "System Updates must not show dev/update explanation as always-visible copy"
    )
    try expect(
        rowsByID["daemonEndpoint"]?.detail == nil,
        "System Daemon Endpoint must render as a compact inspector value row"
    )
    try expect(
        rowsByID["daemonEndpoint"]?.title == "Daemon endpoint"
            && rowsByID["dataRoot"]?.title == "Alan home"
            && rowsByID["applicationSupport"]?.title == "Shell state"
            && rowsByID["shellControl"]?.title == "Control namespace",
        "System path rows must use control-panel labels"
    )
    try expect(
        system.rows.filter { $0.id.hasPrefix("performanceDiagnostics") }.map(\.id)
            == ["performanceDiagnostics", "performanceDiagnosticsExport"],
        "System must preserve diagnostics toggle and export rows"
    )
}

private func requireSection(
    _ id: ShellSettingsSectionID,
    in snapshot: ShellSettingsSurfaceSnapshot
) throws -> ShellSettingsSectionModel {
    guard let section = snapshot.sections.first(where: { $0.id == id }) else {
        throw TestFailure.message("missing settings section \(id.rawValue)")
    }
    return section
}

private func requireNavigationGroup(
    _ id: ShellSettingsNavigationGroup,
    in snapshot: ShellSettingsSurfaceSnapshot
) throws -> ShellSettingsNavigationGroupModel {
    guard let group = snapshot.navigationGroups.first(where: { $0.id == id }) else {
        throw TestFailure.message("missing settings navigation group \(id.rawValue)")
    }
    return group
}

private func helperStatus(
    state: AlanPrivilegedHelperStatusState,
    message: String? = nil
) -> AlanPrivilegedHelperStatus {
    AlanPrivilegedHelperStatus(
        state: state,
        identity: AlanInstallChannel.dev.privilegedHelperIdentity,
        installedVersion: nil,
        expectedVersion: nil,
        sanitizedMessage: message
    )
}

private func stableLocalSummary() -> ShellSettingsLocalSummary {
    ShellSettingsLocalSummary.current(
        channel: .stable,
        environment: [:],
        updateDecision: AlanMacUpdateDecision(
            installation: .direct,
            allowsSparkleUpdates: true,
            menuTitle: "Check for Updates...",
            userMessage: ""
        ),
        homeDirectory: URL(fileURLWithPath: "/Users/test", isDirectory: true)
    )
}

private func testTerminalProfiles() -> TerminalProfileSettingsSummary {
    TerminalProfileSettingsSummary(
        profiles: [
            TerminalProfileDefinition.loginShellFallback,
            TerminalProfileDefinition(
                id: "alan",
                title: "Alan",
                launch: .sudoUser(unixUser: "alan"),
                defaultWorkingDirectory: "/Users/alan",
                presentation: nil
            ),
            TerminalProfileDefinition(
                id: "custom",
                title: "Bootstrap",
                launch: .customCommand("echo hidden-secret"),
                defaultWorkingDirectory: nil,
                presentation: nil
            ),
        ],
        defaultProfileID: "alan",
        recoveryMessage: nil
    )
}

private func devLocalSummary() -> ShellSettingsLocalSummary {
    ShellSettingsLocalSummary.current(
        channel: .dev,
        environment: [:],
        updateDecision: unsupportedDevUpdateDecision(),
        homeDirectory: URL(fileURLWithPath: "/Users/test", isDirectory: true)
    )
}

private func unsupportedDevUpdateDecision() -> AlanMacUpdateDecision {
    AlanMacUpdateDecision(
        installation: .unsupportedChannel,
        allowsSparkleUpdates: false,
        menuTitle: "Check for Updates...",
        userMessage: "This local dev build does not use Sparkle updates."
    )
}

private func makeTemporaryDirectory() throws -> URL {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("alan-shell-settings-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}

private enum ShellSettingsFixtureExporter {
    static func exportIfRequested() throws {
        guard let rootPath = ProcessInfo.processInfo.environment["ALAN_SHELL_SETTINGS_FIXTURE_DIR"],
              !rootPath.isEmpty
        else {
            return
        }

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let rootURL = URL(fileURLWithPath: rootPath)
        for fixture in try fixtures() {
            let fixtureURL = rootURL
                .appendingPathComponent(fixture.id)
                .appendingPathExtension("json")
            try FileManager.default.createDirectory(
                at: fixtureURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try encoder.encode(fixture).write(to: fixtureURL, options: .atomic)
        }
        print("Shell settings summary fixtures exported to \(rootPath).")
    }

    private static func fixtures() throws -> [ShellCoreFixtureCase] {
        let terminalProfiles = TerminalProfileSettingsSummary(
            profiles: [
                TerminalProfileDefinition.loginShellFallback,
                TerminalProfileDefinition(
                    id: "alan",
                    title: "Alan",
                    launch: .sudoUser(unixUser: "alan"),
                    defaultWorkingDirectory: "/Users/alan",
                    presentation: TerminalProfilePresentation(
                        symbolName: "person.crop.circle",
                        colorName: nil
                    ),
                    managedTerminalAccountID: "alan"
                ),
            ],
            defaultProfileID: "login_shell",
            recoveryMessage: "Recovered local store."
        )

        let accountPlan = ManagedTerminalAccountPlanner.plan(
            request: ManagedTerminalAccountRequest(
                accountName: "alan_smoke",
                guiUserName: "morris",
                fullName: "Alan Smoke",
                shell: "/bin/zsh",
                homeDirectory: "/Users/alan_smoke",
                hideFromLoginWindow: true,
                bindCurrentSpaceAfterSuccess: true
            ),
            state: ManagedTerminalAccountState(
                account: .missing,
                sudoers: .missing,
                terminalProfile: .missing,
                verification: .notRun
            )
        )
        let managedAccounts = ManagedTerminalAccountSettingsSummary(plans: [accountPlan])
        let capabilities = ShellSettingsCapabilitiesSummary(
            skills: [
                ShellSettingsSkillSummary(
                    id: "memory",
                    name: "Memory",
                    enabled: true,
                    allowImplicitInvocation: false,
                    available: true
                ),
                ShellSettingsSkillSummary(
                    id: "plan",
                    name: "Plan",
                    enabled: false,
                    allowImplicitInvocation: false,
                    available: true
                ),
            ],
            unavailableReason: nil
        )
        let local = devLocalSummary()
        let diagnostics = ShellSettingsDiagnosticsSummary(
            isEnabled: true,
            retainedEventCount: 7,
            stutterMarkerCount: 1,
            lastExportURL: nil
        )

        return [
            ShellCoreFixtureCase(
                id: "settings-summary/terminal-profile-rows",
                kind: "settings_summary",
                description: "Terminal Profile settings rows preserve default, recovery, profile, and sudo guidance semantics.",
                input: PortableTerminalProfileSettingsSummary(terminalProfiles),
                operation: SettingsOperation(type: "terminal_profile_rows"),
                expected: SettingsRowsExpectation(
                    rows: try sectionRows(
                        .terminalProfiles,
                        terminalProfiles: terminalProfiles
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "settings-summary/managed-account-rows",
                kind: "settings_summary",
                description: "Managed terminal account settings rows summarize plan status without exposing privileged commands.",
                input: PortableManagedTerminalAccountSettingsSummary(managedAccounts),
                operation: SettingsOperation(type: "managed_terminal_account_rows"),
                expected: SettingsRowsExpectation(
                    rows: try sectionRows(
                        .terminalAccounts,
                        managedTerminalAccounts: managedAccounts
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "settings-summary/capability-rows",
                kind: "settings_summary",
                description: "Capability settings rows use compact enabled skill counts.",
                input: PortableCapabilitiesSummary(capabilities),
                operation: SettingsOperation(type: "capability_rows"),
                expected: SettingsRowsExpectation(
                    rows: try sectionRows(.capabilities, capabilities: capabilities)
                )
            ),
            ShellCoreFixtureCase(
                id: "settings-summary/local-diagnostics-rows",
                kind: "settings_summary",
                description: "Local settings rows combine host identity with compact diagnostics metadata.",
                input: PortableLocalDiagnosticsInput(local: local, diagnostics: diagnostics),
                operation: SettingsOperation(type: "local_rows"),
                expected: SettingsRowsExpectation(
                    rows: try sectionRows(.local, local: local, diagnostics: diagnostics)
                )
            ),
        ]
    }

    private static func sectionRows(
        _ sectionID: ShellSettingsSectionID,
        terminalProfiles: TerminalProfileSettingsSummary = testTerminalProfiles(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary = .empty,
        capabilities: ShellSettingsCapabilitiesSummary = .unavailable(reason: "Daemon unavailable"),
        local: ShellSettingsLocalSummary = stableLocalSummary(),
        diagnostics: ShellSettingsDiagnosticsSummary = .disabled
    ) throws -> [ShellSettingsRowModel] {
        let snapshot = ShellSettingsSurfaceSnapshot.make(
            remote: ShellSettingsRemoteSnapshot(
                accounts: .unavailable(reason: "Daemon unavailable"),
                capabilities: capabilities
            ),
            local: local,
            terminalProfiles: terminalProfiles,
            managedTerminalAccounts: managedTerminalAccounts,
            diagnostics: diagnostics
        )
        return try requireSection(sectionID, in: snapshot).rows
    }
}

private struct ShellCoreFixtureCase: Encodable {
    let id: String
    let kind: String
    let source = "swift"
    let description: String
    let input: AnyEncodable
    let operation: AnyEncodable
    let expected: AnyEncodable

    init<Input: Encodable, Operation: Encodable, Expected: Encodable>(
        id: String,
        kind: String,
        description: String,
        input: Input,
        operation: Operation,
        expected: Expected
    ) {
        self.id = id
        self.kind = kind
        self.description = description
        self.input = AnyEncodable(input)
        self.operation = AnyEncodable(operation)
        self.expected = AnyEncodable(expected)
    }
}

private struct AnyEncodable: Encodable {
    private let encodeValue: (Encoder) throws -> Void

    init<Value: Encodable>(_ value: Value) {
        encodeValue = value.encode(to:)
    }

    func encode(to encoder: Encoder) throws {
        try encodeValue(encoder)
    }
}

private struct SettingsOperation: Encodable {
    let type: String
}

private struct SettingsRowsExpectation: Encodable {
    let rows: [PortableSettingsRow]

    init(rows: [ShellSettingsRowModel]) {
        self.rows = rows.map(PortableSettingsRow.init)
    }
}

private struct PortableSettingsRow: Encodable {
    let id: String
    let systemName: String
    let title: String
    let detail: String?
    let value: String?
    let mutability: String
    let offersFreeformEditing: Bool

    init(_ row: ShellSettingsRowModel) {
        id = row.id
        systemName = row.systemName
        title = row.title
        detail = row.detail
        value = row.value
        mutability = mutabilityID(row.mutability)
        offersFreeformEditing = row.offersFreeformEditing
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case systemName = "system_name"
        case title
        case detail
        case value
        case mutability
        case offersFreeformEditing = "offers_freeform_editing"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(systemName, forKey: .systemName)
        try container.encode(title, forKey: .title)
        try container.encodeIfPresent(detail, forKey: .detail)
        try container.encodeIfPresent(value, forKey: .value)
        try container.encode(mutability, forKey: .mutability)
        try container.encode(offersFreeformEditing, forKey: .offersFreeformEditing)
    }
}

private struct PortableTerminalProfileSettingsSummary: Encodable {
    let profiles: [TerminalProfileDefinition]
    let defaultProfileID: String
    let recoveryMessage: String?

    init(_ summary: TerminalProfileSettingsSummary) {
        profiles = summary.profiles
        defaultProfileID = summary.defaultProfileID
        recoveryMessage = summary.recoveryMessage
    }

    private enum CodingKeys: String, CodingKey {
        case profiles
        case defaultProfileID = "default_profile_id"
        case recoveryMessage = "recovery_message"
    }
}

private struct PortableManagedTerminalAccountSettingsSummary: Encodable {
    let plans: [PortableManagedTerminalAccountPlan]

    init(_ summary: ManagedTerminalAccountSettingsSummary) {
        plans = summary.plans.map(PortableManagedTerminalAccountPlan.init)
    }
}

private struct PortableManagedTerminalAccountPlan: Encodable {
    let request: PortableManagedTerminalAccountRequest
    let status: PortableManagedTerminalAccountPlanStatus
    let steps: [PortableManagedTerminalAccountPlanStep]

    init(_ plan: ManagedTerminalAccountPlan) {
        request = PortableManagedTerminalAccountRequest(plan.request)
        status = PortableManagedTerminalAccountPlanStatus(plan.status)
        steps = plan.steps.map(PortableManagedTerminalAccountPlanStep.init)
    }
}

private struct PortableManagedTerminalAccountRequest: Encodable {
    let accountName: String
    let guiUserName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool
    let bindCurrentSpaceAfterSuccess: Bool

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        guiUserName = request.guiUserName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
        bindCurrentSpaceAfterSuccess = request.bindCurrentSpaceAfterSuccess
    }

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case guiUserName = "gui_user_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
        case bindCurrentSpaceAfterSuccess = "bind_current_space_after_success"
    }
}

private struct PortableManagedTerminalAccountPlanStatus: Encodable {
    let type: String
    let path: String?
    let profileID: String?

    init(_ status: ManagedTerminalAccountPlanStatus) {
        switch status {
        case .readyToApply:
            type = "ready_to_apply"
            path = nil
            profileID = nil
        case .alreadyReady:
            type = "already_ready"
            path = nil
            profileID = nil
        case .repair:
            type = "repair"
            path = nil
            profileID = nil
        case .invalid:
            type = "invalid"
            path = nil
            profileID = nil
        case .helperUnavailable:
            type = "helper_unavailable"
            path = nil
            profileID = nil
        case .accountNotAlanManaged:
            type = "account_not_alan_managed"
            path = nil
            profileID = nil
        case let .legacySudoersPresent(path):
            type = "legacy_sudoers_present"
            self.path = path
            profileID = nil
        case .ptySpawnFailed:
            type = "pty_spawn_failed"
            path = nil
            profileID = nil
        case .requiresDestructiveConfirmation:
            type = "requires_destructive_confirmation"
            path = nil
            profileID = nil
        case let .sudoersConflict(path):
            type = "sudoers_conflict"
            self.path = path
            profileID = nil
        case let .terminalProfileConflict(profileID):
            type = "terminal_profile_conflict"
            path = nil
            self.profileID = profileID
        }
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case path
        case profileID = "profile_id"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(type, forKey: .type)
        try container.encodeIfPresent(path, forKey: .path)
        try container.encodeIfPresent(profileID, forKey: .profileID)
    }
}

private struct PortableManagedTerminalAccountPlanStep: Encodable {
    let kind: String
    let summary: String
    let requiresPrivilege: Bool

    init(_ step: ManagedTerminalAccountPlanStep) {
        kind = stepKindID(step.kind)
        summary = step.summary
        requiresPrivilege = step.requiresPrivilege
    }

    private enum CodingKeys: String, CodingKey {
        case kind
        case summary
        case requiresPrivilege = "requires_privilege"
    }
}

private struct PortableCapabilitiesSummary: Encodable {
    let skills: [PortableSkillSummary]
    let unavailableReason: String?

    init(_ summary: ShellSettingsCapabilitiesSummary) {
        skills = summary.skills.map(PortableSkillSummary.init)
        unavailableReason = summary.unavailableReason
    }

    private enum CodingKeys: String, CodingKey {
        case skills
        case unavailableReason = "unavailable_reason"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(skills, forKey: .skills)
        try container.encodeIfPresent(unavailableReason, forKey: .unavailableReason)
    }
}

private struct PortableSkillSummary: Encodable {
    let id: String
    let name: String
    let enabled: Bool
    let allowImplicitInvocation: Bool
    let available: Bool

    init(_ skill: ShellSettingsSkillSummary) {
        id = skill.id
        name = skill.name
        enabled = skill.enabled
        allowImplicitInvocation = skill.allowImplicitInvocation
        available = skill.available
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case enabled
        case allowImplicitInvocation = "allow_implicit_invocation"
        case available
    }
}

private struct PortableLocalDiagnosticsInput: Encodable {
    let local: PortableLocalSummary
    let diagnostics: PortableDiagnosticsSummary

    init(local: ShellSettingsLocalSummary, diagnostics: ShellSettingsDiagnosticsSummary) {
        self.local = PortableLocalSummary(local)
        self.diagnostics = PortableDiagnosticsSummary(diagnostics)
    }
}

private struct PortableLocalSummary: Encodable {
    let bundleIdentifier: String
    let channelLabel: String
    let cliToolName: String
    let daemonURL: String
    let daemonBindAddress: String
    let updateSummary: String
    let updateDetail: String
    let alanHomeDisplayPath: String
    let applicationSupportDisplayPath: String
    let globalSkillsDisplayPath: String
    let shellControlNamespace: String

    init(_ local: ShellSettingsLocalSummary) {
        bundleIdentifier = local.bundleIdentifier
        channelLabel = local.channelLabel
        cliToolName = local.cliToolName
        daemonURL = local.daemonURL
        daemonBindAddress = local.daemonBindAddress
        updateSummary = local.updateSummary
        updateDetail = local.updateDetail
        alanHomeDisplayPath = local.alanHomeDisplayPath
        applicationSupportDisplayPath = local.applicationSupportDisplayPath
        globalSkillsDisplayPath = local.globalSkillsDisplayPath
        shellControlNamespace = local.shellControlNamespace
    }

    private enum CodingKeys: String, CodingKey {
        case bundleIdentifier = "bundle_identifier"
        case channelLabel = "channel_label"
        case cliToolName = "cli_tool_name"
        case daemonURL = "daemon_url"
        case daemonBindAddress = "daemon_bind_address"
        case updateSummary = "update_summary"
        case updateDetail = "update_detail"
        case alanHomeDisplayPath = "alan_home_display_path"
        case applicationSupportDisplayPath = "application_support_display_path"
        case globalSkillsDisplayPath = "global_skills_display_path"
        case shellControlNamespace = "shell_control_namespace"
    }
}

private struct PortableDiagnosticsSummary: Encodable {
    let isEnabled: Bool
    let retainedEventCount: Int
    let stutterMarkerCount: Int
    let lastExportURL: String?

    init(_ diagnostics: ShellSettingsDiagnosticsSummary) {
        isEnabled = diagnostics.isEnabled
        retainedEventCount = diagnostics.retainedEventCount
        stutterMarkerCount = diagnostics.stutterMarkerCount
        lastExportURL = diagnostics.lastExportURL?.path
    }

    private enum CodingKeys: String, CodingKey {
        case isEnabled = "is_enabled"
        case retainedEventCount = "retained_event_count"
        case stutterMarkerCount = "stutter_marker_count"
        case lastExportURL = "last_export_url"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(isEnabled, forKey: .isEnabled)
        try container.encode(retainedEventCount, forKey: .retainedEventCount)
        try container.encode(stutterMarkerCount, forKey: .stutterMarkerCount)
        try container.encodeIfPresent(lastExportURL, forKey: .lastExportURL)
    }
}

private func mutabilityID(_ mutability: ShellSettingsRowMutability) -> String {
    switch mutability {
    case .editable:
        return "editable"
    case .readOnly:
        return "read_only"
    case .actionOnly:
        return "action_only"
    case .deferred:
        return "deferred"
    }
}

private func stepKindID(_ kind: ManagedTerminalAccountPlanStepKind) -> String {
    switch kind {
    case .createStandardAccount:
        return "create_standard_account"
    case .repairAccountType:
        return "repair_account_type"
    case .repairHomeDirectory:
        return "repair_home_directory"
    case .repairShell:
        return "repair_shell"
    case .hideAccount:
        return "hide_account"
    case .writeSudoersDropIn:
        return "write_sudoers_drop_in"
    case .validateSudoers:
        return "validate_sudoers"
    case .verifyTerminalEntry:
        return "verify_terminal_entry"
    case .createOrUpdateTerminalProfile:
        return "create_or_update_terminal_profile"
    case .bindCurrentSpace:
        return "bind_current_space"
    case .removeSudoersDropIn:
        return "remove_sudoers_drop_in"
    case .removeManagedTerminalProfile:
        return "remove_managed_terminal_profile"
    case .deleteAccount:
        return "delete_account"
    case .deleteHomeDirectory:
        return "delete_home_directory"
    case .helperStep(let kind):
        return kind.rawValue
    }
}

private func invokeXPCStatus(
    service: AlanPrivilegedHelperXPCService,
    request: AlanPrivilegedHelperXPCRequest
) throws -> AlanPrivilegedHelperXPCResponse {
    try invokeXPCStatus(
        service: service,
        rawRequest: AlanPrivilegedHelperXPCCodec.encode(request)
    )
}

private func invokeXPCStatus(
    service: AlanPrivilegedHelperXPCService,
    rawRequest: NSData
) throws -> AlanPrivilegedHelperXPCResponse {
    var reply: NSData?
    service.helperStatus(rawRequest) { reply = $0 }
    guard let reply else {
        throw TestFailure.message("helper XPC service must synchronously reply in focused tests")
    }
    return try AlanPrivilegedHelperXPCCodec.decodeResponse(reply)
}

private func invokeXPCPerform(
    service: AlanPrivilegedHelperXPCService,
    request: AlanPrivilegedHelperXPCRequest
) throws -> AlanPrivilegedHelperXPCResponse {
    var reply: NSData?
    service.performRequest(AlanPrivilegedHelperXPCCodec.encode(request)) { reply = $0 }
    guard let reply else {
        throw TestFailure.message("helper XPC service must synchronously reply in focused tests")
    }
    return try AlanPrivilegedHelperXPCCodec.decodeResponse(reply)
}

private func decodedPayload<T: Decodable>(
    _ response: AlanPrivilegedHelperXPCResponse
) throws -> T {
    guard let payload = response.payload else {
        throw TestFailure.message("helper XPC response must include a typed payload")
    }
    return try JSONDecoder().decode(T.self, from: payload)
}

private func alanManagedOwnership(_ accountName: String) -> ManagedTerminalAccountOwnershipState {
    .alanManaged(
        .helperMarker(
            path: "/Library/Application Support/alan-macos/privileged-helper/managed-users/\(accountName)/ownership.json"
        )
    )
}

private func helperDiagnosis(
    accountName: String,
    readiness: AlanManagedUserReadinessState,
    ownership: AlanManagedUserOwnershipState,
    legacySudoersPath: String? = nil,
    terminalProfileID: String? = nil,
    ptySmokeVerified: Bool = false,
    homeDirectoryExists: Bool = true,
    shellMatches: Bool = true,
    hiddenFromLoginWindow: Bool = true
) -> AlanManagedUserDiagnosis {
    let request = ManagedTerminalAccountRequest(
        accountName: accountName,
        guiUserName: "morris",
        fullName: accountName
    )
    return AlanManagedUserDiagnosis(
        request: request,
        ownershipState: ownership,
        readinessState: readiness,
        accountExists: readiness != .accountMissing,
        homeDirectoryExists: homeDirectoryExists,
        shellMatches: shellMatches,
        hiddenFromLoginWindow: hiddenFromLoginWindow,
        legacySudoersPath: legacySudoersPath,
        terminalProfileID: terminalProfileID,
        ptySmokeVerified: ptySmokeVerified,
        diagnostic: nil
    )
}

private func readRepositoryFile(_ path: String) throws -> String {
    try String(contentsOfFile: path, encoding: .utf8)
}

private func sourceSlice(
    named start: String,
    in source: String,
    endingBefore end: String
) throws -> String {
    guard let startRange = source.range(of: start),
          let endRange = source[startRange.upperBound...].range(of: end)
    else {
        throw TestFailure.message("Could not locate source slice \(start)")
    }
    return String(source[startRange.lowerBound..<endRange.lowerBound])
}

private struct StubManagedTerminalAccountCommandRunner: ManagedTerminalAccountCommandRunning {
    let responses: [String: ManagedTerminalAccountCommandResult]

    func run(
        executablePath: String,
        arguments: [String]
    ) -> ManagedTerminalAccountCommandResult {
        responses[([executablePath] + arguments).joined(separator: " ")]
            ?? ManagedTerminalAccountCommandResult(
                exitCode: 1,
                standardOutput: "",
                standardError: "stubbed command not found"
            )
    }
}

private struct StubSudoersSyntaxChecker: ManagedTerminalAccountSudoersSyntaxChecking {
    let result: ManagedTerminalAccountSudoersValidationResult

    func validateSudoersFile(atPath path: String) -> ManagedTerminalAccountSudoersValidationResult {
        result
    }
}

private struct StubTerminalEntryVerifier: ManagedTerminalAccountEntryVerifying {
    let result: ManagedTerminalAccountSudoersValidationResult

    func verifyTerminalEntry(
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountSudoersValidationResult {
        result
    }
}

private final class SudoersFixtureFileManager: FileManager {
    private let files: [String: String]
    private let existingPaths: Set<String>

    init(files: [String: String] = [:], existingPaths: Set<String> = []) {
        self.files = files
        self.existingPaths = existingPaths
        super.init()
    }

    override func fileExists(atPath path: String) -> Bool {
        files[path] != nil || existingPaths.contains(path)
    }

    override func contents(atPath path: String) -> Data? {
        files[path]?.data(using: .utf8)
    }
}
#endif
