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
            try testLocalRowsStayBounded()
            try testDevChannelLocalRowsUseDevIdentity()
            try testPrivilegedHelperIdentityIsChannelScoped()
            try testPrivilegedHelperCurrentIdentityUsesLaunchdServiceName()
            try testPrivilegedHelperLifecycleServiceUsesSMAppServiceIdentityAndFakeStates()
            try testPrivilegedHelperSettingsRowsExposeLifecycleStates()
            try testPrivilegedHelperXPCBoundaryIsTypedAndChannelScoped()
            try testPrivilegedHelperPtyInputPreservesShortWrites()
            try testPrivilegedHelperManagedUserApplyUsesLongTimeout()
            try testPrivilegedHelperRevalidatesOwnershipBeforeDestructiveDeletes()
            try testPrivilegedHelperRequestValidationIsNarrowAndSanitized()
            try testLocalFolderOpenerRequiresExistingDirectory()
            try testManagedUsersUseCurrentHelperContract()
            try testRetiredManagedProfileRequiresExplicitRepair()
            try testManagedUserRollbackAndProfileSelectionStayBounded()
            try testPerformanceDiagnosticsRowsAreCompactAndLocal()
            try testNavigationGroupsMapTaskOrientedRows()
            try testNavigationGroupsPlaceSystemRows()
            print("Shell settings surface tests passed.")
        } catch {
            fputs("Shell settings surface tests failed: \(error)\n", stderr)
            exit(1)
        }
    }
}

private func testDefaultSectionOrderAndInterfaceMutability() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )

    try expect(
        snapshot.sections.map(\.id) == ShellSettingsSectionID.defaultOrder,
        "settings sections must render local shell surfaces in canonical order"
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

private func testLocalRowsStayBounded() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let visibleText = snapshot.visibleText.joined(separator: "\n")

    try expect(
        !visibleText.localizedCaseInsensitiveContains("daemon")
            && !visibleText.localizedCaseInsensitiveContains("http"),
        "local Settings must not expose retired host-service contracts"
    )

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
        local: devLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let localText = try requireSection(.local, in: snapshot).visibleText.joined(separator: "\n")

    try expect(
        localText.contains("app.alanworks.macos.dev"),
        "dev settings must show the Alan Dev bundle identifier"
    )
    try expect(localText.contains("alan-dev"), "dev settings must show the alan-dev CLI tool")
    try expect(
        localText.contains("~/Library/Application Support/Alan/System Store/dev"),
        "dev settings must show the channel System Store"
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
    let protocolStatus: AlanPrivilegedHelperProtocolStatus? = try decodedPayload(response)
    try expect(
        protocolStatus?.protocolVersion == AlanPrivilegedHelperProtocolStatus.currentVersion,
        "helper status must identify the current wire protocol"
    )
    try expect(
        AlanPrivilegedHelperStatus.fromXPCStatus(
            response,
            identity: AlanInstallChannel.dev.privilegedHelperIdentity(
                signingTeamIdentifier: "TEAMID1234"
            )
        ).state == .healthy,
        "current helper protocol must be healthy"
    )
    let oldHelperResponse = AlanPrivilegedHelperXPCResponse.accepted(
        request: request,
        identity: identity,
        message: "Privileged helper XPC boundary is available.",
        payload: try JSONEncoder().encode(
            AlanPrivilegedHelperProtocolStatus(protocolVersion: 2)
        )
    )
    let oldHelperStatus = AlanPrivilegedHelperStatus.fromXPCStatus(
        oldHelperResponse,
        identity: AlanInstallChannel.dev.privilegedHelperIdentity(
            signingTeamIdentifier: "TEAMID1234"
        )
    )
    try expect(
        oldHelperStatus.state == .outdated
            && oldHelperStatus.installedVersion == "2"
            && oldHelperStatus.expectedVersion
                == String(AlanPrivilegedHelperProtocolStatus.currentVersion),
        "outdated helper responses must require the current protocol before diagnosis"
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
    try expect(
        diagnosis.homeDirectoryMatches == false,
        "helper XPC diagnose must carry configured-home match state"
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
                    workingDirectory: "/tmp/bad user",
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
        "clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperPTYSessionStore.swift"
    )
    let sessionStore = try sourceSlice(
        named: "final class AlanPrivilegedHelperPTYSessionStore",
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

private func testPrivilegedHelperManagedUserApplyUsesLongTimeout() throws {
    let client = try readRepositoryFile(
        "clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPCClient.swift"
    )

    try expect(
        client.contains("managedUserApplyTimeoutSeconds: TimeInterval = 600")
            && client.contains("case .applyManagedUserPlan:")
            && client.contains("return max(timeoutSeconds, Self.managedUserApplyTimeoutSeconds)"),
        "managed_user helper apply XPC requests must use the long apply budget instead of the 5s default"
    )
}

private func testPrivilegedHelperRevalidatesOwnershipBeforeDestructiveDeletes() throws {
    let helperSource = try readRepositoryFile(
        "clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperManagedUserService.swift"
    )
    let apply = try sourceSlice(
        named: "func apply(\n        plan: AlanXPCManagedUserHelperPlan,",
        in: helperSource,
        endingBefore: "func removeIntegration"
    )

    try expect(
        apply.contains("var destructiveAccountRecord: AlanManagedUserAccountRecord?")
            && apply.contains("case .deleteAccount:")
            && apply.contains("managedAccountRecordForDestructiveDeletion(plan.request)")
            && apply.contains("destructiveAccountRecord = account")
            && apply.contains("case .deleteHomeDirectory:")
            && apply.contains("validateHomeDeletionStillManaged(")
            && apply.contains("destructiveOwnershipEvidenceExists(for: request)")
            && apply.contains("currentAccount.uid != originalAccount.uid"),
        "helper apply must revalidate Alan ownership before destructive account/home deletes"
    )
}

private func testPrivilegedHelperRequestValidationIsNarrowAndSanitized() throws {
    let valid = ManagedTerminalAccountRequest(
        accountName: "lab",
        fullName: "Lab User"
    )
    try expect(
        AlanPrivilegedHelperRequestValidator.validate(request: valid, channel: .dev).isEmpty,
        "valid helper requests must pass the narrow account/home/shell contract"
    )

    let invalid = ManagedTerminalAccountRequest(
        accountName: "bad user",
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

private func testManagedUsersUseCurrentHelperContract() throws {
    let request = ManagedTerminalAccountRequest(accountName: "lab", fullName: "Lab User")
    let profiles = TerminalProfileSettingsSummary(
        profiles: [
            .loginShellFallback,
            TerminalProfileDefinition(
                id: "manual",
                title: "Manual sudo profile",
                launch: .sudoUser(unixUser: "operator"),
                defaultWorkingDirectory: nil,
                presentation: nil
            ),
        ],
        defaultProfileID: "login_shell",
        recoveryMessage: nil
    )
    let diagnosis = helperDiagnosis(
        accountName: request.accountName,
        readiness: .accountMissing,
        ownership: .missing,
        homeDirectoryExists: false,
        shellMatches: false,
        hiddenFromLoginWindow: false
    )
    let plan = ManagedTerminalAccountPlanner.plan(
        request: request,
        diagnosis: diagnosis,
        terminalProfiles: profiles.document
    )

    try expect(plan.status == .readyToApply, "a missing Managed User must be ready to apply")
    try expect(
        plan.steps.map(\.kind) == [
            .helperStep(.createStandardAccount),
            .helperStep(.hideAccount),
            .helperStep(.writeOwnershipMarker),
            .helperStep(.verifyAccount),
            .helperStep(.verifyManagedUserPTY),
            .createOrUpdateTerminalProfile,
        ],
        "Managed User creation must use only current helper and profile handoff steps"
    )

    let snapshot = ShellSettingsSurfaceSnapshot.make(
        local: devLocalSummary(),
        terminalProfiles: profiles,
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary(plans: [plan])
    )
    let visibleText = snapshot.visibleText.joined(separator: "\n").lowercased()
    try expect(
        !visibleText.contains("sudoers") && !visibleText.contains("legacy cleanup"),
        "Settings must not expose retired Managed User compatibility state"
    )
    try expect(
        profiles.profiles.first(where: { $0.id == "manual" })?.launch == .sudoUser(unixUser: "operator"),
        "manually authored sudo_user profiles must remain operator-owned"
    )
}

private func testRetiredManagedProfileRequiresExplicitRepair() throws {
    let request = ManagedTerminalAccountRequest(accountName: "lab", fullName: "Lab User")
    let retiredProfile = TerminalProfileDefinition(
        id: "lab",
        title: "Lab User",
        launch: .sudoUser(unixUser: "lab"),
        defaultWorkingDirectory: "/Users/lab",
        presentation: nil,
        managedTerminalAccountID: "lab"
    )
    let profiles = TerminalProfileSettingsSummary(
        profiles: [.loginShellFallback, retiredProfile],
        defaultProfileID: "login_shell",
        recoveryMessage: nil
    )
    let plan = ManagedTerminalAccountPlanner.plan(
        request: request,
        diagnosis: helperDiagnosis(
            accountName: request.accountName,
            readiness: .ready,
            ownership: .alanManaged,
            terminalProfileID: request.terminalProfileID,
            ptySmokeVerified: true
        ),
        terminalProfiles: profiles.document
    )

    try expect(
        plan.status == .readyToApply && plan.steps.map(\.kind) == [.createOrUpdateTerminalProfile],
        "a retired managed sudo_user profile must require explicit profile repair"
    )
    try expect(
        profiles.document.profile(id: "lab")?.launch == .sudoUser(unixUser: "lab"),
        "planning must not migrate a retired profile while loading it"
    )
}

private func testManagedUserRollbackAndProfileSelectionStayBounded() throws {
    let readyRequest = ManagedTerminalAccountRequest(accountName: "ready", fullName: "Ready User")
    let repairRequest = ManagedTerminalAccountRequest(accountName: "repair", fullName: "Repair User")
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
            TerminalProfileDefinition(
                id: "manual",
                title: "Manual",
                launch: .sudoRoot,
                defaultWorkingDirectory: nil,
                presentation: nil
            ),
            TerminalProfileDefinition(
                id: "repair",
                title: "Repair User",
                launch: .managedUser(unixUser: "repair"),
                defaultWorkingDirectory: "/Users/repair",
                presentation: nil,
                managedTerminalAccountID: "repair"
            ),
        ],
        defaultProfileID: "login_shell",
        recoveryMessage: nil
    )
    let readyDiagnosis = helperDiagnosis(
        accountName: readyRequest.accountName,
        readiness: .ready,
        ownership: .alanManaged,
        terminalProfileID: readyRequest.terminalProfileID,
        ptySmokeVerified: true
    )
    let managed = ManagedTerminalAccountSettingsSummary(plans: [
        ManagedTerminalAccountPlanner.plan(
            request: readyRequest,
            diagnosis: readyDiagnosis,
            terminalProfiles: profiles.document
        ),
        ManagedTerminalAccountPlanner.plan(
            request: repairRequest,
            diagnosis: helperDiagnosis(
                accountName: repairRequest.accountName,
                readiness: .repairable,
                ownership: .alanManaged
            ),
            terminalProfiles: profiles.document
        ),
    ])
    try expect(
        TerminalProfileSpaceIdentityFilter.selectableProfiles(
            terminalProfiles: profiles,
            managedTerminalAccounts: managed
        ).map(\.id) == ["ready", "manual"],
        "Space identity choices must include ready Managed Users and manual profiles only"
    )

    let integrationOnly = ManagedTerminalAccountPlanner.rollbackPlan(
        request: readyRequest,
        diagnosis: readyDiagnosis,
        scope: .alanIntegrationOnly,
        terminalProfiles: profiles.document
    )
    try expect(
        integrationOnly.steps.map(\.kind) == [
            .removeManagedTerminalProfile,
            .helperStep(.removeManagedUserIntegration),
        ],
        "ordinary rollback must remove only current Alan integration"
    )
    let destructive = ManagedTerminalAccountPlanner.rollbackPlan(
        request: readyRequest,
        diagnosis: readyDiagnosis,
        scope: .deleteAccountAndHome(confirmation: nil),
        terminalProfiles: profiles.document
    )
    try expect(
        destructive.status == .requiresDestructiveConfirmation,
        "account and home deletion must remain separately confirmed"
    )
}

private func testPerformanceDiagnosticsRowsAreCompactAndLocal() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
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
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let groups = snapshot.navigationGroups

    try expect(
        groups.map(\.id) == [.general, .terminal, .system],
        "settings navigation groups must use General, Terminal, and System"
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

private func testNavigationGroupsPlaceSystemRows() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        local: stableLocalSummary(),
        terminalProfiles: testTerminalProfiles(),
        diagnostics: ShellSettingsDiagnosticsSummary(
            isEnabled: false,
            retainedEventCount: 24,
            stutterMarkerCount: 2,
            lastExportURL: nil
        )
    )
    let system = try requireNavigationGroup(.system, in: snapshot)

    try expect(
        ["appIdentity", "installChannel", "updates", "cliTool", "dataRoot",
         "applicationSupport", "shellControl"].allSatisfy(system.rows.map(\.id).contains),
        "System must contain local app, storage, and shell control rows"
    )
    let rowsByID = Dictionary(uniqueKeysWithValues: system.rows.map { ($0.id, $0) })
    try expect(
        rowsByID["updates"]?.detail == nil,
        "System Updates must not show dev/update explanation as always-visible copy"
    )
    try expect(
        rowsByID["dataRoot"]?.title == "Alan OS data"
            && rowsByID["applicationSupport"]?.title == "Shell state"
            && rowsByID["shellControl"]?.title == "Control namespace",
        "System path rows must use control-panel labels"
    )
    try expect(
        !system.visibleText.joined(separator: " ").localizedCaseInsensitiveContains("daemon"),
        "System Settings must not retain daemon-era labels"
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
                    launch: .managedUser(unixUser: "alan"),
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
                fullName: "Alan Smoke",
                shell: "/bin/zsh",
                homeDirectory: "/Users/alan_smoke",
                hideFromLoginWindow: true
            ),
            diagnosis: helperDiagnosis(
                accountName: "alan_smoke",
                readiness: .accountMissing,
                ownership: .missing,
                homeDirectoryExists: false,
                shellMatches: false,
                hiddenFromLoginWindow: false
            ),
            terminalProfiles: terminalProfiles.document
        )
        let managedAccounts = ManagedTerminalAccountSettingsSummary(plans: [accountPlan])
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
        local: ShellSettingsLocalSummary = stableLocalSummary(),
        diagnostics: ShellSettingsDiagnosticsSummary = .disabled
    ) throws -> [ShellSettingsRowModel] {
        let snapshot = ShellSettingsSurfaceSnapshot.make(
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
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
    }

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
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
        case .ptySpawnFailed:
            type = "pty_spawn_failed"
            path = nil
            profileID = nil
        case .requiresDestructiveConfirmation:
            type = "requires_destructive_confirmation"
            path = nil
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
    let updateSummary: String
    let updateDetail: String
    let systemStoreDisplayPath: String
    let applicationSupportDisplayPath: String
    let shellControlNamespace: String

    init(_ local: ShellSettingsLocalSummary) {
        bundleIdentifier = local.bundleIdentifier
        channelLabel = local.channelLabel
        cliToolName = local.cliToolName
        updateSummary = local.updateSummary
        updateDetail = local.updateDetail
        systemStoreDisplayPath = local.systemStoreDisplayPath
        applicationSupportDisplayPath = local.applicationSupportDisplayPath
        shellControlNamespace = local.shellControlNamespace
    }

    private enum CodingKeys: String, CodingKey {
        case bundleIdentifier = "bundle_identifier"
        case channelLabel = "channel_label"
        case cliToolName = "cli_tool_name"
        case updateSummary = "update_summary"
        case updateDetail = "update_detail"
        case systemStoreDisplayPath = "system_store_display_path"
        case applicationSupportDisplayPath = "application_support_display_path"
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
    case .createOrUpdateTerminalProfile:
        return "create_or_update_terminal_profile"
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

private func helperDiagnosis(
    accountName: String,
    readiness: AlanManagedUserReadinessState,
    ownership: AlanManagedUserOwnershipState,
    terminalProfileID: String? = nil,
    ptySmokeVerified: Bool = false,
    isAdmin: Bool = false,
    homeDirectoryExists: Bool = true,
    homeDirectoryMatches: Bool = true,
    shellMatches: Bool = true,
    hiddenFromLoginWindow: Bool = true
) -> AlanManagedUserDiagnosis {
    let request = ManagedTerminalAccountRequest(
        accountName: accountName,
        fullName: accountName
    )
    return AlanManagedUserDiagnosis(
        request: request,
        ownershipState: ownership,
        readinessState: readiness,
        accountExists: readiness != .accountMissing,
        isAdmin: isAdmin,
        homeDirectoryExists: homeDirectoryExists,
        homeDirectoryMatches: homeDirectoryMatches,
        shellMatches: shellMatches,
        hiddenFromLoginWindow: hiddenFromLoginWindow,
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

#endif
