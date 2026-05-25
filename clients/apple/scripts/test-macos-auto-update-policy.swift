import Foundation

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

private func temporaryDirectory(_ name: String) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("alan-auto-update-policy-tests", isDirectory: true)
        .appendingPathComponent(name, isDirectory: true)
    try? FileManager.default.removeItem(at: root)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
}

private func createExecutable(at url: URL) throws {
    try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    try "#!/usr/bin/env bash\n".write(to: url, atomically: true, encoding: .utf8)
}

private func testStableDirectInstallUsesSparkle() throws {
    let root = try temporaryDirectory("direct")
    let app = root.appendingPathComponent("Applications/Alan.app", isDirectory: true)
    let decision = AlanMacUpdatePolicy.decision(
        appBundleURL: app,
        channel: .stable,
        homebrewPrefixes: [root.appendingPathComponent("homebrew").path],
        fileManager: .default
    )

    try expect(decision.installation == .direct, "stable app outside Homebrew must be direct")
    try expect(decision.allowsSparkleUpdates, "stable direct install must allow Sparkle updates")
    try expect(decision.menuTitle == "Check for Updates...", "direct install must expose normal update command")
}

private func testDevChannelDoesNotUseSparkle() throws {
    let root = try temporaryDirectory("dev")
    let app = root.appendingPathComponent("Applications/Alan Dev.app", isDirectory: true)
    let decision = AlanMacUpdatePolicy.decision(
        appBundleURL: app,
        channel: .dev,
        homebrewPrefixes: [],
        fileManager: .default
    )

    try expect(decision.installation == .unsupportedChannel, "dev app must not use stable Sparkle feed")
    try expect(!decision.allowsSparkleUpdates, "dev app must not allow Sparkle replacement")
    try expect(
        decision.userMessage.contains("local dev build"),
        "dev channel message must explain why Sparkle is disabled"
    )
}

private func testHomebrewCaskroomPathDoesNotUseSparkle() throws {
    let root = try temporaryDirectory("homebrew-path")
    let prefix = root.appendingPathComponent("opt/homebrew", isDirectory: true)
    let app = prefix.appendingPathComponent("Caskroom/alan/0.1.0/Alan.app", isDirectory: true)
    let decision = AlanMacUpdatePolicy.decision(
        appBundleURL: app,
        channel: .stable,
        homebrewPrefixes: [prefix.path],
        fileManager: .default
    )

    try expect(decision.installation == .homebrewManaged, "Caskroom app path must be Homebrew-managed")
    try expect(!decision.allowsSparkleUpdates, "Homebrew-managed app must not allow Sparkle replacement")
    try expect(
        decision.userMessage.contains("brew upgrade --cask alan"),
        "Homebrew-managed update message must point at brew upgrade"
    )
}

private func testHomebrewResolvedCaskroomPathDoesNotUseSparkle() throws {
    let root = try temporaryDirectory("homebrew-symlinked-app")
    let prefix = root.appendingPathComponent("opt/homebrew", isDirectory: true)
    let caskApp = prefix.appendingPathComponent("Caskroom/alan/0.1.0/Alan.app", isDirectory: true)
    let app = root.appendingPathComponent("Applications/Alan.app", isDirectory: true)

    try FileManager.default.createDirectory(
        at: caskApp,
        withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
        at: app.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    try FileManager.default.createSymbolicLink(at: app, withDestinationURL: caskApp)

    let decision = AlanMacUpdatePolicy.decision(
        appBundleURL: app,
        channel: .stable,
        homebrewPrefixes: [prefix.path],
        fileManager: .default
    )

    try expect(decision.installation == .homebrewManaged, "resolved Caskroom app path must be Homebrew-managed")
    try expect(!decision.allowsSparkleUpdates, "resolved Caskroom app path must disable Sparkle")
}

private func testDirectCommandLinkDoesNotDisableSparkle() throws {
    let root = try temporaryDirectory("direct-link")
    let prefix = root.appendingPathComponent("opt/homebrew", isDirectory: true)
    let app = root.appendingPathComponent("Applications/Alan.app", isDirectory: true)
    let embeddedAlan = app.appendingPathComponent("Contents/Resources/bin/alan")
    let homebrewAlan = prefix.appendingPathComponent("bin/alan")

    try createExecutable(at: embeddedAlan)
    try FileManager.default.createDirectory(
        at: homebrewAlan.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    try FileManager.default.createSymbolicLink(at: homebrewAlan, withDestinationURL: embeddedAlan)

    let decision = AlanMacUpdatePolicy.decision(
        appBundleURL: app,
        channel: .stable,
        homebrewPrefixes: [prefix.path],
        fileManager: .default
    )

    try expect(decision.installation == .direct, "command link alone must not mark a direct app Homebrew-managed")
    try expect(decision.allowsSparkleUpdates, "direct app with its own command link must still allow Sparkle")
}

@main
private enum TestRunner {
    static func run() throws {
        try testStableDirectInstallUsesSparkle()
        try testDevChannelDoesNotUseSparkle()
        try testHomebrewCaskroomPathDoesNotUseSparkle()
        try testHomebrewResolvedCaskroomPathDoesNotUseSparkle()
        try testDirectCommandLinkDoesNotDisableSparkle()
        print("macOS auto-update policy tests passed.")
    }

    static func main() {
        do {
            try run()
        } catch {
            fputs("Test failed: \(error)\n", stderr)
            exit(1)
        }
    }
}
