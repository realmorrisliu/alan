import Darwin
import Foundation

#if os(macOS)
@main
struct TerminalRuntimeServiceTestRunner {
    static func main() async {
        await MainActor.run {
            TerminalRuntimeServiceTests.run()
        }
    }
}

@MainActor
private enum TerminalRuntimeServiceTests {
    static func run() {
        verifiesGhosttyTerminfoEnvironmentProjection()
        verifiesRuntimeCwdDoesNotRequireSurfaceRecreation()
        verifiesInstallDiscoveryChangesDoNotRequireSurfaceRecreation()
        verifiesBootstrapReuseAndPaneHandleIdentity()
        verifiesPaneScopedHandleIsolation()
        verifiesContentScopedHandleSurvivesPaneRemount()
        verifiesDeliveryAndMissingRuntimeResults()
        verifiesDeliveryRejectsExitedRuntime()
        verifiesQueuedAndTimeoutDeliveryStates()
        verifiesControlResponseCarriesDeliveryDiagnostics()
        verifiesTeardownOnce()
        verifiesFinalizePanesOnlyReleasesStaleHandles()
        verifiesBootstrapFailureDiagnostics()
        print("Terminal runtime service tests passed.")
    }

    private static func verifiesGhosttyTerminfoEnvironmentProjection() {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("alan-ghostty-terminfo-\(UUID().uuidString)", isDirectory: true)
        try! FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        setenv("ALAN_GHOSTTY_TERMINFO_DIR", tempDir.path, 1)
        defer {
            unsetenv("ALAN_GHOSTTY_TERMINFO_DIR")
            try? FileManager.default.removeItem(at: tempDir)
        }

        let state = ShellStateSnapshot.bootstrapDefault()
        let pane = state.panes[0]
        let profile = AlanShellBootProfile.forPane(pane, shellState: state)

        expect(
            profile.environment["TERMINFO"] == tempDir.path,
            "boot profile must pass Ghostty terminfo to terminal child processes"
        )
        expect(
            profile.environment["TERM_PROGRAM"] == "alan",
            "boot profile must identify alan as the terminal program"
        )
        expect(
            profile.environment["COLORTERM"] == "truecolor",
            "boot profile must advertise truecolor terminal support"
        )
        expect(
            profile.environment["ALAN_SHELL_CONTENT_ID"] == pane.terminalContentID,
            "boot profile must expose terminal content identity to child processes"
        )
    }

    private static func verifiesRuntimeCwdDoesNotRequireSurfaceRecreation() {
        let base = sampleBootProfile(workingDirectory: "/Users/morris")
        let afterCd = sampleBootProfile(workingDirectory: "/Users/morris/Developer/Alan")
        let rediscoveredEnvironment = sampleBootProfile(
            workingDirectory: "/Users/morris/Developer/Alan",
            environment: ["TERMINFO": "/tmp/other-terminfo"]
        )

        expect(
            !afterCd.requiresSurfaceRecreation(comparedTo: base),
            "runtime cwd updates must not recreate the Ghostty surface"
        )
        expect(
            !rediscoveredEnvironment.requiresSurfaceRecreation(comparedTo: base),
            "terminal environment rediscovery must not recreate the Ghostty surface"
        )
        expect(
            base.requiresSurfaceRecreation(comparedTo: nil),
            "missing previous boot profile must require initial surface creation"
        )
    }

    private static func verifiesInstallDiscoveryChangesDoNotRequireSurfaceRecreation() {
        let running = sampleBootProfile(
            workingDirectory: "/Users/morris",
            environment: ["TERMINFO": "/Users/morris/Applications/Alan.app/Contents/Resources/ghostty-terminfo"],
            ghostty: GhosttyIntegrationStatus(
                frameworkPath: "/Users/morris/Applications/Alan.app/Contents/Resources/GhosttyKit.xcframework",
                resourcesPath: "/Users/morris/Applications/Alan.app/Contents/Resources/ghostty-resources",
                terminfoPath: "/Users/morris/Applications/Alan.app/Contents/Resources/ghostty-terminfo",
                candidates: []
            )
        )
        let whileBundleIsBeingReplaced = sampleBootProfile(
            workingDirectory: "/Users/morris",
            environment: [:],
            ghostty: GhosttyIntegrationStatus(
                frameworkPath: nil,
                resourcesPath: nil,
                terminfoPath: nil,
                candidates: []
            )
        )
        let alanFromBundle = sampleBootProfile(
            workingDirectory: "/Users/morris",
            command: sampleAlanCommand(
                strategy: .bundledResourceBinary,
                executablePath: "/Users/morris/Applications/Alan.app/Contents/Resources/bin/alan"
            ),
            environment: [
                "ALAN_SHELL_LAUNCH_TARGET": ShellLaunchTarget.alan.rawValue,
                "ALAN_SHELL_EXECUTABLE": "/Users/morris/Applications/Alan.app/Contents/Resources/bin/alan",
            ]
        )
        let alanDuringInstall = sampleBootProfile(
            workingDirectory: "/Users/morris",
            command: AlanCommandResolution(
                strategy: .shellLookup,
                executablePath: nil,
                launchPath: "/bin/zsh",
                arguments: ["-lc", "alan chat"],
                bootCommand: "alan chat",
                surfaceCommand: "alan chat",
                summary: "No direct alan binary found; falling back to shell PATH lookup",
                detail: nil,
                repoRoot: nil,
                candidates: []
            ),
            environment: [
                "ALAN_SHELL_LAUNCH_TARGET": ShellLaunchTarget.alan.rawValue
            ]
        )

        expect(
            !whileBundleIsBeingReplaced.requiresSurfaceRecreation(comparedTo: running),
            "install-time Ghostty resource discovery changes must not recreate a running surface"
        )
        expect(
            !alanDuringInstall.requiresSurfaceRecreation(comparedTo: alanFromBundle),
            "install-time alan binary discovery changes must not recreate a running surface"
        )
    }

    private static func verifiesBootstrapReuseAndPaneHandleIdentity() {
        let bootstrap = FakeAlanGhosttyProcessBootstrap()
        let service = AlanWindowTerminalRuntimeService(
            bootstrap: bootstrap,
            surfaceFactory: { contentID, paneID, _ in
                FakeAlanTerminalSurfaceHandle(contentID: contentID, paneID: paneID)
            }
        )

        let first = service.surfaceHandle(for: "pane_1", bootProfile: nil)
        let second = service.surfaceHandle(for: "pane_1", bootProfile: nil)
        expect(first === second, "service must preserve pane handle identity")
        expect(bootstrap.ensureCallCount == 1, "bootstrap must run once for repeated pane lookup")

        let secondWindow = AlanWindowTerminalRuntimeService(
            bootstrap: bootstrap,
            surfaceFactory: { contentID, paneID, _ in
                FakeAlanTerminalSurfaceHandle(contentID: contentID, paneID: paneID)
            }
        )
        secondWindow.ensureReady()
        expect(bootstrap.ensureCallCount == 1, "shared bootstrap must not reinitialize per window")
    }

    private static func verifiesPaneScopedHandleIsolation() {
        let service = FakeAlanTerminalRuntimeService()
        let first = service.surfaceHandle(for: "pane_1", bootProfile: nil)
        let second = service.surfaceHandle(for: "pane_2", bootProfile: nil)

        expect(first !== second, "different panes must receive distinct service-owned handles")
        expect(first.paneID == "pane_1", "first handle must retain its pane identity")
        expect(second.paneID == "pane_2", "second handle must retain its pane identity")
        expect(
            service.registeredPaneIDs == ["pane_1", "pane_2"],
            "service must expose registered pane identities"
        )
        expect(
            service.snapshot(for: "pane_1")?.paneID == "pane_1",
            "snapshots must remain available through pane convenience lookup"
        )
        expect(
            service.snapshot(for: "pane_2")?.paneID == "pane_2",
            "snapshots must remain available through pane convenience lookup"
        )
    }

    private static func verifiesContentScopedHandleSurvivesPaneRemount() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_terminal_primary"
        let first = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_left",
            bootProfile: nil
        )
        let second = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_right",
            bootProfile: nil
        )

        expect(first === second, "same terminal content must reuse the service-owned handle")
        expect(second.contentID == contentID, "handle must retain terminal content identity")
        expect(second.paneID == "pane_right", "handle must update to the latest PaneSlot mount")
        expect(service.registeredContentIDs == [contentID], "service registration must be content keyed")
        expect(service.registeredPaneIDs == ["pane_right"], "pane registration must reflect the current mount")

        let accepted = service.sendText(toTerminalContentID: contentID, text: "after move")
        let handle = second as! FakeAlanTerminalSurfaceHandle
        expect(accepted.applied, "content-keyed delivery must reach the remounted handle")
        expect(handle.deliveredText == ["after move"], "delivery must stay bound to content identity")
        expect(
            service.snapshot(forTerminalContentID: contentID)?.contentID == contentID,
            "snapshot lookup must be content keyed"
        )
        expect(
            service.snapshot(forTerminalContentID: contentID)?.paneID == "pane_right",
            "snapshot must project the latest PaneSlot mount"
        )
    }

    private static func verifiesDeliveryAndMissingRuntimeResults() {
        let service = FakeAlanTerminalRuntimeService()
        let handle = service.surfaceHandle(for: "pane_1", bootProfile: nil) as! FakeAlanTerminalSurfaceHandle

        let accepted = service.sendText(to: "pane_1", text: "hello")
        expect(accepted.applied, "accepted delivery must report applied")
        expect(accepted.acceptedBytes == 5, "accepted delivery must report utf8 byte count")
        expect(handle.deliveredText == ["hello"], "fake handle must observe delivered text")

        let missing = service.sendText(to: "pane_missing", text: "hello")
        expect(missing.code == .missingTarget, "missing pane must report runtime-missing")
        expect(missing.applied == false, "missing pane must not report applied")
    }

    private static func verifiesDeliveryRejectsExitedRuntime() {
        let service = FakeAlanTerminalRuntimeService()
        let handle = service.surfaceHandle(for: "pane_1", bootProfile: nil) as! FakeAlanTerminalSurfaceHandle
        handle.markProcessExited(exitCode: 0)

        let rejected = service.sendText(to: "pane_1", text: "after exit")

        expect(rejected.applied == false, "exited runtime delivery must not report applied")
        expect(rejected.errorCode == "terminal_child_exited", "exited runtime delivery must use stable error code")
        expect(handle.deliveredText.isEmpty, "exited runtime delivery must not reach the surface")
    }

    private static func verifiesQueuedAndTimeoutDeliveryStates() {
        let service = FakeAlanTerminalRuntimeService()
        let handle = service.surfaceHandle(for: "pane_1", bootProfile: nil) as! FakeAlanTerminalSurfaceHandle
        handle.deliveryResult = .queued(byteCount: 5, runtimePhase: "attachable")

        let queued = service.sendText(to: "pane_1", text: "hello")
        expect(queued.code == .queued, "queued delivery must preserve queued state")
        expect(queued.acceptedBytes == 5, "queued delivery must preserve byte count")
        expect(queued.runtimePhase == "attachable", "queued delivery must preserve runtime phase")

        let timeout = TerminalRuntimeDeliveryResult.timeout(
            errorMessage: "runtime command exceeded deadline",
            runtimePhase: "bootstrapping"
        )
        expect(timeout.code == .timeout, "timeout delivery must preserve timeout state")
        expect(timeout.errorCode == "terminal_runtime_timeout", "timeout delivery must be stable")
        expect(timeout.runtimePhase == "bootstrapping", "timeout delivery must preserve phase")
    }

    private static func verifiesControlResponseCarriesDeliveryDiagnostics() {
        let response = AlanShellControlResponse(
            requestID: "req_1",
            contractVersion: "0.1",
            applied: false,
            state: nil,
            spaces: nil,
            tabs: nil,
            panes: nil,
            pane: nil,
            items: nil,
            candidates: nil,
            events: nil,
            focusedPaneID: nil,
            spaceID: "space_1",
            tabID: "tab_1",
            paneID: "pane_1",
            acceptedBytes: 0,
            deliveryCode: TerminalRuntimeDeliveryCode.missingTarget.rawValue,
            runtimePhase: "failed",
            latestEventID: nil,
            errorCode: "terminal_runtime_missing",
            errorMessage: "missing"
        )

        let data = try! JSONEncoder().encode(response)
        let json = String(decoding: data, as: UTF8.self)
        expect(json.contains("\"delivery_code\":\"missing_target\""), "control response must encode delivery code")
        expect(json.contains("\"runtime_phase\":\"failed\""), "control response must encode runtime phase")
    }

    private static func verifiesTeardownOnce() {
        let service = FakeAlanTerminalRuntimeService()
        let handle = service.surfaceHandle(for: "pane_1", bootProfile: nil) as! FakeAlanTerminalSurfaceHandle

        expect(service.finalizePane("pane_1") == .completed, "first finalize must complete")
        expect(handle.teardownCount == 1, "first finalize must tear down exactly once")
        expect(service.finalizePane("pane_1") == .notStarted, "missing second finalize must be stable")
        expect(handle.teardownCount == 1, "second finalize must not repeat teardown")
    }

    private static func verifiesFinalizePanesOnlyReleasesStaleHandles() {
        let service = FakeAlanTerminalRuntimeService()
        let active = service.surfaceHandle(
            for: "pane_active",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle
        let stale = service.surfaceHandle(
            for: "pane_stale",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle

        service.finalizePanes(excluding: ["pane_active"])

        expect(active.teardownCount == 0, "active pane handle must not be finalized")
        expect(stale.teardownCount == 1, "stale pane handle must be finalized")
        expect(
            service.existingSurfaceHandle(for: "pane_active") === active,
            "active pane handle must remain registered"
        )
        expect(
            service.existingSurfaceHandle(for: "pane_stale") == nil,
            "stale pane handle must be removed"
        )
    }

    private static func verifiesBootstrapFailureDiagnostics() {
        let failedDiagnostics = AlanGhosttyBootstrapDiagnostics(
            phase: .failed,
            summary: "Fake Ghostty bootstrap failed.",
            detail: nil,
            failureReason: "missing resources",
            dependencies: GhosttyIntegrationStatus.discover(),
            lastUpdatedAt: .now
        )
        let bootstrap = FakeAlanGhosttyProcessBootstrap(nextDiagnostics: failedDiagnostics)
        let service = AlanWindowTerminalRuntimeService(
            bootstrap: bootstrap,
            surfaceFactory: { contentID, paneID, _ in
                FakeAlanTerminalSurfaceHandle(contentID: contentID, paneID: paneID)
            }
        )

        let diagnostics = service.ensureReady()
        expect(diagnostics.phase == .failed, "failed bootstrap must publish failed phase")
        expect(diagnostics.failureReason == "missing resources", "failed bootstrap must retain reason")
    }

    private static func sampleBootProfile(
        workingDirectory: String,
        command: AlanCommandResolution? = nil,
        environment: [String: String] = ["TERMINFO": "/tmp/ghostty-terminfo"],
        ghostty: GhosttyIntegrationStatus = GhosttyIntegrationStatus(
            frameworkPath: "/tmp/GhosttyKit.xcframework",
            resourcesPath: "/tmp/ghostty-resources",
            terminfoPath: "/tmp/ghostty-terminfo",
            candidates: []
        )
    ) -> AlanShellBootProfile {
        AlanShellBootProfile(
            command: command ?? sampleShellCommand(),
            workingDirectory: workingDirectory,
            environment: environment,
            ghostty: ghostty
        )
    }

    private static func sampleShellCommand() -> AlanCommandResolution {
        AlanCommandResolution(
            strategy: .loginShellFallback,
            executablePath: "/bin/zsh",
            launchPath: "/bin/zsh",
            arguments: ["-l"],
            bootCommand: "/bin/zsh -l",
            surfaceCommand: nil,
            summary: "Launching pane with the default login shell",
            detail: "/bin/zsh",
            repoRoot: nil,
            candidates: []
        )
    }

    private static func sampleAlanCommand(
        strategy: AlanLaunchStrategy,
        executablePath: String
    ) -> AlanCommandResolution {
        AlanCommandResolution(
            strategy: strategy,
            executablePath: executablePath,
            launchPath: executablePath,
            arguments: ["chat"],
            bootCommand: "\(executablePath) chat",
            surfaceCommand: "\(executablePath) chat",
            summary: "Launching alan",
            detail: executablePath,
            repoRoot: nil,
            candidates: []
        )
    }

    private static func expect(
        _ condition: @autoclosure () -> Bool,
        _ message: String
    ) {
        guard condition() else {
            fputs("error: \(message)\n", stderr)
            exit(1)
        }
    }
}
#endif
