import Foundation

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
            try testLocalSummaryReadsHostConfigForDaemonEndpoint()
            try testWorkspaceContextUsesRegistryForWorkspaceScopedRequests()
            try testWorkspaceContextFallsBackToDiscoveredWorkspaceRoot()
            try testUnavailableRemoteSummariesStayCompact()
            try testTerminalProfilesAndAccountsStayLocalAndRedacted()
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

    for sectionID in [ShellSettingsSectionID.accounts, .capabilities, .local] {
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
}

private func testDevChannelLocalRowsUseDevIdentity() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Daemon unavailable"),
        local: devLocalSummary(),
        terminalProfiles: testTerminalProfiles()
    )
    let localText = try requireSection(.local, in: snapshot).visibleText.joined(separator: "\n")

    try expect(localText.contains("Alan Dev"), "dev settings must identify the Alan Dev app")
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
    let profileSection = try requireSection(.terminalProfiles, in: snapshot)
    let accountSection = try requireSection(.terminalAccounts, in: snapshot)
    let providerSection = try requireSection(.accounts, in: snapshot)
    let visibleText = snapshot.visibleText.joined(separator: "\n")

    try expect(
        profileSection.visibleText.contains("Terminal Profiles"),
        "Terminal Profiles must render as local startup configuration"
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

private func requireSection(
    _ id: ShellSettingsSectionID,
    in snapshot: ShellSettingsSurfaceSnapshot
) throws -> ShellSettingsSectionModel {
    guard let section = snapshot.sections.first(where: { $0.id == id }) else {
        throw TestFailure.message("missing settings section \(id.rawValue)")
    }
    return section
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
#endif
