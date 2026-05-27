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
            try testUnavailableRemoteSummariesStayCompact()
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
        local: stableLocalSummary()
    )

    try expect(
        snapshot.sections.map(\.id) == ShellSettingsSectionID.defaultOrder,
        "settings sections must render in Interface, Accounts, Sessions, Capabilities, Local order"
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
        local: stableLocalSummary()
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
        local: devLocalSummary()
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

private func testUnavailableRemoteSummariesStayCompact() throws {
    let snapshot = ShellSettingsSurfaceSnapshot.make(
        remote: .unavailable(reason: "Connection refused"),
        local: stableLocalSummary()
    )
    let text = snapshot.visibleText.joined(separator: "\n")

    try expect(text.contains("Unavailable"), "unavailable remote sources must render a compact state")
    try expect(!text.contains("Error("), "unavailable state must not render raw debug payloads")
    try expect(
        !text.contains("thinking_budget_tokens"),
        "sessions summary must not expose deprecated thinking budget controls"
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

private func devLocalSummary() -> ShellSettingsLocalSummary {
    ShellSettingsLocalSummary.current(
        channel: .dev,
        environment: [:],
        updateDecision: AlanMacUpdateDecision(
            installation: .unsupportedChannel,
            allowsSparkleUpdates: false,
            menuTitle: "Check for Updates...",
            userMessage: "This local dev build does not use Sparkle updates."
        ),
        homeDirectory: URL(fileURLWithPath: "/Users/test", isDirectory: true)
    )
}
#endif
