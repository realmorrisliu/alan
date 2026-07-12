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

private func require(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() {
        throw TestFailure.message(message)
    }
}

private func temporaryDirectory(_ name: String) throws -> URL {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("alan-cli-installer-tests", isDirectory: true)
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
        .appendingPathComponent(name, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}

private func makeResourceRoot(channel: AlanInstallChannel = .stable) throws -> URL {
    let appRoot = try temporaryDirectory(channel.ownedAppBundleNames[0])
    return try makeResourceRoot(appRoot: appRoot, channel: channel)
}

private func makeResourceRoot(appRoot: URL, channel: AlanInstallChannel = .stable) throws -> URL {
    let root = appRoot
        .appendingPathComponent("Contents", isDirectory: true)
        .appendingPathComponent("Resources", isDirectory: true)
    let bin = root.appendingPathComponent("bin", isDirectory: true)
    try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)

    for tool in channel.toolNames {
        let url = bin.appendingPathComponent(tool)
        try "#!/bin/sh\nexit 0\n".write(to: url, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }

    return root
}

private func testDevChannelInstallsDevToolNames() throws {
    let resourceRoot = try makeResourceRoot(channel: .dev)
    let targetDirectory = try temporaryDirectory("dev-target")

    let records = try AlanCommandLineToolInstaller.install(
        targetDirectory: targetDirectory,
        resourceURL: resourceRoot,
        channel: .dev
    )

    try require(records.map(\.tool) == ["alan-dev"], "dev installer must report dev tool")
    for tool in AlanInstallChannel.dev.toolNames {
        let target = targetDirectory.appendingPathComponent(tool)
        let destination = try FileManager.default.destinationOfSymbolicLink(atPath: target.path)
        try require(
            destination.hasSuffix("/bin/\(tool)"),
            "dev installer must link \(tool) to the embedded resource"
        )
    }
    try require(
        !FileManager.default.fileExists(atPath: targetDirectory.appendingPathComponent("alan").path),
        "dev installer must not create stable alan link"
    )
}

private func testInstallsSymlinks() throws {
    let resourceRoot = try makeResourceRoot()
    let targetDirectory = try temporaryDirectory("target")

    let records = try AlanCommandLineToolInstaller.install(
        targetDirectory: targetDirectory,
        resourceURL: resourceRoot
    )

    try require(records.count == 1, "installer must report alan")
    for tool in AlanCommandLineToolInstaller.toolNames {
        let target = targetDirectory.appendingPathComponent(tool)
        let destination = try FileManager.default.destinationOfSymbolicLink(atPath: target.path)
        try require(
            destination.hasSuffix("/bin/\(tool)"),
            "installer must link \(tool) to the embedded resource"
        )
    }
}

private func testSkipsNonAlanFiles() throws {
    let resourceRoot = try makeResourceRoot()
    let targetDirectory = try temporaryDirectory("existing")
    let existing = targetDirectory.appendingPathComponent("alan")
    try "not alan\n".write(to: existing, atomically: true, encoding: .utf8)

    let records = try AlanCommandLineToolInstaller.install(
        targetDirectory: targetDirectory,
        resourceURL: resourceRoot
    )
    let alan = records.first { $0.tool == "alan" }

    guard case .skipped = alan?.status else {
        throw TestFailure.message("installer must skip a non-alan existing file")
    }
}

private func testRejectsHomebrewPrefixTarget() throws {
    let resourceRoot = try makeResourceRoot()
    let homebrewPrefix = try temporaryDirectory("homebrew")
    let targetDirectory = homebrewPrefix.appendingPathComponent("bin", isDirectory: true)

    do {
        _ = try AlanCommandLineToolInstaller.install(
            targetDirectory: targetDirectory,
            resourceURL: resourceRoot,
            homebrewPrefixes: [homebrewPrefix.path]
        )
        throw TestFailure.message("installer must reject Homebrew-managed targets")
    } catch let error as CocoaError where error.code == .fileWriteNoPermission {
        _ = error
        return
    }
}

private func testSkipsWhenHomebrewAlreadyManagesLinks() throws {
    let directResourceRoot = try makeResourceRoot()
    let targetDirectory = try temporaryDirectory("target")
    let homebrewPrefix = try temporaryDirectory("homebrew")
    let homebrewApp = homebrewPrefix
        .appendingPathComponent("Caskroom/alan/0.1.0/Alan.app", isDirectory: true)
    let homebrewResourceRoot = try makeResourceRoot(appRoot: homebrewApp)
    let homebrewBin = homebrewPrefix.appendingPathComponent("bin", isDirectory: true)
    try FileManager.default.createDirectory(at: homebrewBin, withIntermediateDirectories: true)

    for tool in AlanCommandLineToolInstaller.toolNames {
        let source = homebrewResourceRoot
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(tool)
        let link = homebrewBin.appendingPathComponent(tool)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: source)
    }

    let records = try AlanCommandLineToolInstaller.install(
        targetDirectory: targetDirectory,
        resourceURL: directResourceRoot,
        homebrewPrefixes: [homebrewPrefix.path]
    )

    try require(records.count == 1, "installer must report alan")
    for record in records {
        guard case .skipped(let reason) = record.status else {
            throw TestFailure.message("installer must skip when Homebrew already owns links")
        }
        try require(
            reason.contains("Homebrew already manages"),
            "skip reason must explain Homebrew ownership"
        )
        let alternateTarget = targetDirectory.appendingPathComponent(record.tool)
        try require(
            !FileManager.default.fileExists(atPath: alternateTarget.path),
            "installer must not create alternate PATH links when Homebrew owns \(record.tool)"
        )
    }
}

private func testDirectAppLinkInHomebrewPrefixDoesNotSkipInstall() throws {
    let resourceRoot = try makeResourceRoot()
    let targetDirectory = try temporaryDirectory("target")
    let homebrewPrefix = try temporaryDirectory("homebrew")
    let homebrewBin = homebrewPrefix.appendingPathComponent("bin", isDirectory: true)
    try FileManager.default.createDirectory(at: homebrewBin, withIntermediateDirectories: true)

    for tool in AlanCommandLineToolInstaller.toolNames {
        let source = resourceRoot
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(tool)
        let link = homebrewBin.appendingPathComponent(tool)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: source)
    }

    let records = try AlanCommandLineToolInstaller.install(
        targetDirectory: targetDirectory,
        resourceURL: resourceRoot,
        homebrewPrefixes: [homebrewPrefix.path]
    )

    try require(records.count == 1, "installer must report alan")
    for record in records {
        guard case .installed = record.status else {
            throw TestFailure.message("direct app command link must not be treated as Homebrew ownership")
        }
        try require(
            FileManager.default.fileExists(atPath: targetDirectory.appendingPathComponent(record.tool).path),
            "installer must still create the requested direct-install link"
        )
    }
}

private func testLeavesLowercaseAppLinksUntouched() throws {
    let resourceRoot = try makeResourceRoot()
    let targetDirectory = try temporaryDirectory("target")
    try FileManager.default.createDirectory(at: targetDirectory, withIntermediateDirectories: true)

    for tool in AlanCommandLineToolInstaller.toolNames {
        let legacyDestination = URL(fileURLWithPath: "/Applications/alan.app")
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("Resources", isDirectory: true)
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(tool)
        let link = targetDirectory.appendingPathComponent(tool)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: legacyDestination)
    }

    let records = try AlanCommandLineToolInstaller.install(
        targetDirectory: targetDirectory,
        resourceURL: resourceRoot
    )

    try require(records.count == 1, "installer must report alan")
    for record in records {
        guard case .skipped = record.status else {
            throw TestFailure.message("lowercase app link must not be treated as Alan-owned")
        }
    }
    for tool in AlanCommandLineToolInstaller.toolNames {
        let target = targetDirectory.appendingPathComponent(tool)
        let destination = try FileManager.default.destinationOfSymbolicLink(atPath: target.path)
        try require(
            destination.contains("/alan.app/Contents/Resources/bin/\(tool)"),
            "installer must leave an unrelated lowercase app link untouched for \(tool)"
        )
    }
}

private func testRejectsAlanHomeBinTarget() throws {
    let resourceRoot = try makeResourceRoot()
    let targetDirectory = try temporaryDirectory("home")
        .appendingPathComponent(".alan", isDirectory: true)
        .appendingPathComponent("bin", isDirectory: true)

    do {
        _ = try AlanCommandLineToolInstaller.install(
            targetDirectory: targetDirectory,
            resourceURL: resourceRoot
        )
        throw TestFailure.message("installer must reject ~/.alan/bin-style targets")
    } catch let error as CocoaError where error.code == .fileWriteInvalidFileName {
        _ = error
        return
    }
}

private func testRejectsDevAlanHomeBinTarget() throws {
    let resourceRoot = try makeResourceRoot(channel: .dev)
    let targetDirectory = try temporaryDirectory("dev-home")
        .appendingPathComponent(".alan-dev", isDirectory: true)
        .appendingPathComponent("bin", isDirectory: true)

    do {
        _ = try AlanCommandLineToolInstaller.install(
            targetDirectory: targetDirectory,
            resourceURL: resourceRoot,
            channel: .dev
        )
        throw TestFailure.message("dev installer must reject ~/.alan-dev/bin-style targets")
    } catch let error as CocoaError where error.code == .fileWriteInvalidFileName {
        _ = error
        return
    }
}

private func testChannelResolvesFromBundleIdentifier() throws {
    try require(
        AlanInstallChannel.fromBundleIdentifier("app.alanworks.macos") == .stable,
        "stable bundle id must resolve to stable channel"
    )
    try require(
        AlanInstallChannel.fromBundleIdentifier("app.alanworks.macos.dev") == .dev,
        "dev bundle id must resolve to dev channel"
    )
    try require(
        AlanInstallChannel.fromBundleIdentifier("example.invalid") == nil,
        "unknown bundle id must not resolve to an install channel"
    )
}

private func testChannelDescriptorsExposeRuntimeIsolationIdentity() throws {
    try require(
        AlanInstallChannel.stable.bundleIdentifier == "app.alanworks.macos",
        "stable bundle identifier must preserve public app identity"
    )
    try require(
        AlanInstallChannel.dev.bundleIdentifier == "app.alanworks.macos.dev",
        "dev bundle identifier must expose local dev app identity"
    )
    try require(
        AlanInstallChannel.stable.applicationSupportDirectoryName == "alan-macos",
        "stable support directory must remain compatible"
    )
    try require(
        AlanInstallChannel.dev.applicationSupportDirectoryName == "alan-macos-dev",
        "dev support directory must be channel-scoped"
    )
    try require(
        AlanInstallChannel.stable.shellControlNamespace == "alan-shell-control",
        "stable shell-control namespace must remain compatible"
    )
    try require(
        AlanInstallChannel.dev.shellControlNamespace == "alan-dev-shell-control",
        "dev shell-control namespace must be channel-scoped"
    )
    try require(
        AlanInstallChannel.dev.logSubsystem == "app.alanworks.macos.dev",
        "dev log subsystem must be channel-scoped"
    )
}

@main
private enum TestRunner {
    static func main() throws {
        try testInstallsSymlinks()
        try testDevChannelInstallsDevToolNames()
        try testSkipsNonAlanFiles()
        try testRejectsHomebrewPrefixTarget()
        try testSkipsWhenHomebrewAlreadyManagesLinks()
        try testDirectAppLinkInHomebrewPrefixDoesNotSkipInstall()
        try testLeavesLowercaseAppLinksUntouched()
        try testRejectsAlanHomeBinTarget()
        try testRejectsDevAlanHomeBinTarget()
        try testChannelResolvesFromBundleIdentifier()
        try testChannelDescriptorsExposeRuntimeIsolationIdentity()
    }
}
