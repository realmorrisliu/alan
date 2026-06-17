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
            try testLocalSummaryReadsHostConfigForDaemonEndpoint()
            try testLocalFolderOpenerRequiresExistingDirectory()
            try testWorkspaceContextUsesRegistryForWorkspaceScopedRequests()
            try testWorkspaceContextFallsBackToDiscoveredWorkspaceRoot()
            try testUnavailableRemoteSummariesStayCompact()
            try testTerminalProfilesAndAccountsStayLocalAndRedacted()
            try testPerformanceDiagnosticsRowsAreCompactAndLocal()
            try testNavigationGroupsMapTaskOrientedRows()
            try testNavigationGroupsKeepTerminalIdentityOutOfAgent()
            try testNavigationGroupsPlaceAgentAndSystemRows()
            try ShellSettingsFixtureExporter.exportIfRequested()
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
        terminalRowsByID["terminalProfilesDefault"]?.title == "Default profile"
            && terminalRowsByID["terminalProfilesDefault"]?.detail == "Used for new terminals.",
        "Terminal default profile row must use the shared setting label/detail/value template"
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
        "Terminal must contain the default Terminal Profile row"
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
    }
}
#endif
