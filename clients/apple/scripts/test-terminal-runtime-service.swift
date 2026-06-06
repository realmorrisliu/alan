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
        verifiesDevChannelPropagatesInstallChannelEnvironment()
        verifiesChannelScopedShellControlPaths()
        verifiesDevBootProfileUsesDevShellControlNamespace()
        verifiesBootstrapReuseAndPaneHandleIdentity()
        verifiesPaneScopedHandleIsolation()
        verifiesContentScopedHandleSurvivesPaneRemount()
        verifiesContentScopedDeliveryPreservesPendingDiagnostics()
        verifiesDeliveryAndMissingRuntimeResults()
        verifiesDeliveryRejectsExitedRuntime()
        verifiesExitedRuntimeTakesPrecedenceOverUnavailableRuntime()
        verifiesUnavailableRuntimeDoesNotDeliverTextAndRecordsSnapshot()
        verifiesQueuedAndTimeoutDeliveryStates()
        verifiesControlResponseCarriesDeliveryDiagnostics()
        verifiesRuntimeServiceCapturesLiveTranscriptSnapshot()
        verifiesRuntimeServiceUsesRingBufferFallbackAndReportsFailures()
        verifiesRuntimeServiceSeedsRestoredTranscriptBeforeInput()
        verifiesRuntimeServiceClearsRestoredTranscriptCache()
        verifiesFinalizeEvictsRestoredTranscriptCache()
        verifiesTeardownOnce()
        verifiesFinalizePanesOnlyReleasesStaleHandles()
        verifiesBootstrapFailureDiagnostics()
        verifiesTerminalRenderPriorityDerivation()
        verifiesRenderCoordinatorCoalescesHiddenWakeups()
        verifiesRenderCoordinatorDrainsForegroundBeforeBackground()
        verifiesRenderCoordinatorCatchUpRefreshesHiddenSurface()
        verifiesRenderCoordinatorRecordsDiagnosticsWithoutChangingDrainBehavior()
        verifiesHiddenRuntimePublicationPolicyThrottlesNoisyUpdates()
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

    private static func verifiesTerminalRenderPriorityDerivation() {
        let visiblePaneIDs: Set<String> = ["pane_1", "pane_2"]

        expect(
            terminalRuntimeRenderPriority(
                paneID: "pane_1",
                paneSpaceID: "space_1",
                paneTabID: "tab_1",
                selectedSpaceID: "space_1",
                selectedTabID: "tab_1",
                focusedPaneID: "pane_1",
                visiblePaneIDs: visiblePaneIDs,
                windowIsVisible: true
            ) == .foregroundInteractive,
            "focused selected visible pane must be foreground interactive"
        )
        expect(
            terminalRuntimeRenderPriority(
                paneID: "pane_2",
                paneSpaceID: "space_1",
                paneTabID: "tab_1",
                selectedSpaceID: "space_1",
                selectedTabID: "tab_1",
                focusedPaneID: "pane_1",
                visiblePaneIDs: visiblePaneIDs,
                windowIsVisible: true
            ) == .visibleBackground,
            "visible split sibling must be visible background"
        )
        expect(
            terminalRuntimeRenderPriority(
                paneID: "pane_3",
                paneSpaceID: "space_1",
                paneTabID: "tab_2",
                selectedSpaceID: "space_1",
                selectedTabID: "tab_1",
                focusedPaneID: "pane_1",
                visiblePaneIDs: visiblePaneIDs,
                windowIsVisible: true
            ) == .hiddenBackground,
            "terminal in a hidden tab must be hidden background"
        )
        expect(
            terminalRuntimeRenderPriority(
                paneID: "pane_1",
                paneSpaceID: "space_1",
                paneTabID: "tab_1",
                selectedSpaceID: "space_1",
                selectedTabID: "tab_1",
                focusedPaneID: "pane_1",
                visiblePaneIDs: visiblePaneIDs,
                windowIsVisible: false
            ) == .hiddenBackground,
            "occluded window must make visible panes hidden for rendering"
        )
    }

    private static func verifiesRenderCoordinatorCoalescesHiddenWakeups() {
        var events: [String] = []
        let coordinator = TerminalRenderCoordinator(automaticallyDrains: false)
        let hidden = FakeRenderCoordinatorHost(
            id: "hidden",
            priority: .hiddenBackground,
            events: { events.append($0) }
        )

        coordinator.requestWakeup(from: hidden)
        coordinator.requestWakeup(from: hidden)
        coordinator.drainPending()

        expect(hidden.appTickCount == 1, "hidden wakeups in one interval must coalesce to one app tick")
        expect(hidden.surfaceRefreshCount == 0, "hidden wakeups must not repaint on every output burst")
        let metrics = coordinator.metricsSnapshot()
        expect(
            metrics.coalescedSurfaceRefreshes == 1,
            "coordinator metrics must record the hidden refresh coalescing"
        )
        expect(metrics.drainBatches == 1, "coordinator metrics must count drain batches")
        expect(
            metrics.maxDrainLatencyMs >= 0,
            "coordinator metrics must record drain latency"
        )
        expect(events == ["tick:hidden"], "hidden drain must only process app tick state")
    }

    private static func verifiesRenderCoordinatorDrainsForegroundBeforeBackground() {
        var events: [String] = []
        let coordinator = TerminalRenderCoordinator(automaticallyDrains: false)
        let hidden = FakeRenderCoordinatorHost(
            id: "hidden",
            priority: .hiddenBackground,
            events: { events.append($0) }
        )
        let foreground = FakeRenderCoordinatorHost(
            id: "foreground",
            priority: .foregroundInteractive,
            events: { events.append($0) }
        )
        let visible = FakeRenderCoordinatorHost(
            id: "visible",
            priority: .visibleBackground,
            events: { events.append($0) }
        )

        coordinator.requestWakeup(from: hidden)
        coordinator.requestWakeup(from: visible)
        coordinator.requestWakeup(from: foreground)
        coordinator.drainPending()

        expect(
            events == [
                "tick:foreground", "refresh:foreground:automatic",
                "tick:visible", "refresh:visible:automatic",
                "tick:hidden",
            ],
            "coordinator must drain foreground, visible background, then hidden background"
        )
        expect(
            coordinator.metricsSnapshot().maxDrainBatchSize == 3,
            "coordinator metrics must record the largest coalesced drain batch"
        )
    }

    private static func verifiesRenderCoordinatorCatchUpRefreshesHiddenSurface() {
        var events: [String] = []
        let coordinator = TerminalRenderCoordinator(automaticallyDrains: false)
        let hidden = FakeRenderCoordinatorHost(
            id: "hidden",
            priority: .hiddenBackground,
            events: { events.append($0) }
        )

        coordinator.requestWakeup(from: hidden)
        coordinator.drainPending()
        hidden.priority = .visibleBackground
        coordinator.requestCatchUp(from: hidden)
        coordinator.drainPending()

        expect(hidden.appTickCount == 2, "catch-up must drain current app state")
        expect(hidden.surfaceRefreshCount == 1, "catch-up must refresh the existing surface")
        expect(
            events == ["tick:hidden", "tick:hidden", "refresh:hidden:catch_up"],
            "catch-up must refresh only after the terminal becomes visible"
        )
        expect(coordinator.metricsSnapshot().catchUpRefreshes == 1, "catch-up refresh must be counted")
    }

    private static func verifiesRenderCoordinatorRecordsDiagnosticsWithoutChangingDrainBehavior() {
        var events: [String] = []
        let recorder = AlanPerformanceDiagnosticsRecorder(
            configuration: AlanPerformanceDiagnosticsConfiguration(maxEvents: 16)
        )
        recorder.setEnabled(true)
        let coordinator = TerminalRenderCoordinator(
            automaticallyDrains: false,
            diagnosticsRecorder: recorder
        )
        let foreground = FakeRenderCoordinatorHost(
            id: "foreground",
            priority: .foregroundInteractive,
            events: { events.append($0) }
        )
        let hidden = FakeRenderCoordinatorHost(
            id: "hidden",
            priority: .hiddenBackground,
            events: { events.append($0) }
        )

        coordinator.requestWakeup(from: hidden)
        coordinator.requestWakeup(from: foreground)
        coordinator.requestCatchUp(from: hidden)
        coordinator.drainPending()

        expect(
            events == [
                "tick:foreground", "refresh:foreground:automatic",
                "tick:hidden", "refresh:hidden:catch_up",
            ],
            "diagnostic probes must not change coordinator drain order or refresh decisions"
        )

        let diagnostics = recorder.eventsSnapshot()
        let diagnosticKinds = diagnostics.map(\.kind)
        expect(diagnosticKinds.contains(.ghosttyWakeup), "coordinator must record wakeup diagnostics")
        expect(diagnosticKinds.contains(.ghosttyAppTick), "coordinator must record app tick diagnostics")
        expect(
            diagnosticKinds.contains(.ghosttySurfaceRefresh),
            "coordinator must record surface refresh diagnostics"
        )
        expect(
            diagnosticKinds.contains(.terminalCatchUpRefresh),
            "coordinator must record catch-up refresh diagnostics"
        )
        expect(
            diagnostics.contains {
                $0.priority == "foregroundInteractive" && $0.visibility == "visible"
            },
            "coordinator diagnostics must include render priority and visibility"
        )

        AlanPerformanceDiagnosticsController.shared.setEnabled(false)
        AlanPerformanceDiagnosticsController.shared.setEnabled(true)
        defer { AlanPerformanceDiagnosticsController.shared.setEnabled(false) }
        let sharedCoordinator = TerminalRenderCoordinator(automaticallyDrains: false)
        let sharedHost = FakeRenderCoordinatorHost(
            id: "shared",
            priority: .foregroundInteractive,
            events: { _ in }
        )
        sharedCoordinator.requestWakeup(from: sharedHost)
        sharedCoordinator.drainPending()

        let sharedKinds = AlanPerformanceDiagnosticsController.shared.eventsSnapshot().map(\.kind)
        expect(
            sharedKinds.contains(.ghosttyAppTick),
            "app render coordinator must record Ghostty diagnostics into the shared controller"
        )
    }

    private static func verifiesHiddenRuntimePublicationPolicyThrottlesNoisyUpdates() {
        let previous = sampleRuntimeSnapshot(
            priority: .hiddenBackground,
            metadata: .placeholder,
            lastUpdatedAt: Date(timeIntervalSince1970: 1)
        )
        let timestampOnly = sampleRuntimeSnapshot(
            priority: .hiddenBackground,
            metadata: .placeholder,
            lastUpdatedAt: Date(timeIntervalSince1970: 2)
        )
        expect(
            !TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: previous,
                next: timestampOnly
            ),
            "hidden runtime timestamp-only churn must not publish to shell UI"
        )

        let rendererPhaseChurn = sampleRuntimeSnapshot(
            priority: .hiddenBackground,
            metadata: .placeholder,
            renderer: TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .firstRefresh,
                summary: "Terminal surface refreshed.",
                detail: nil,
                failureReason: nil,
                recentEvents: ["refresh"]
            ),
            lastUpdatedAt: Date(timeIntervalSince1970: 3)
        )
        expect(
            !TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: previous,
                next: rendererPhaseChurn
            ),
            "hidden runtime renderer phase churn must not publish to shell UI"
        )

        let rendererFailure = sampleRuntimeSnapshot(
            priority: .hiddenBackground,
            metadata: .placeholder,
            renderer: TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Terminal renderer failed.",
                detail: nil,
                failureReason: "renderer_failed",
                recentEvents: ["failed"]
            ),
            lastUpdatedAt: Date(timeIntervalSince1970: 4)
        )
        expect(
            TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: previous,
                next: rendererFailure
            ),
            "hidden runtime renderer failures must remain publishable"
        )

        let titleChange = sampleRuntimeSnapshot(
            priority: .hiddenBackground,
            metadata: TerminalPaneMetadataSnapshot(
                title: "cargo test",
                workingDirectory: nil,
                summary: nil,
                attention: .idle,
                processExited: false,
                lastCommandExitCode: nil,
                lastUpdatedAt: Date(timeIntervalSince1970: 5)
            ),
            lastUpdatedAt: Date(timeIntervalSince1970: 5)
        )
        expect(
            TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: previous,
                next: titleChange
            ),
            "hidden runtime title changes must remain publishable for sidebar summaries"
        )

        let foreground = sampleRuntimeSnapshot(
            priority: .foregroundInteractive,
            metadata: .placeholder,
            lastUpdatedAt: Date(timeIntervalSince1970: 6)
        )
        expect(
            TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: previous,
                next: foreground
            ),
            "foreground runtime updates must publish immediately"
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

        expect(
            !whileBundleIsBeingReplaced.requiresSurfaceRecreation(comparedTo: running),
            "install-time Ghostty resource discovery changes must not recreate a running surface"
        )
    }

    private static func verifiesDevChannelPropagatesInstallChannelEnvironment() {
        let previousInstallChannel = ProcessInfo.processInfo.environment["ALAN_INSTALL_CHANNEL"]
        setenv("ALAN_INSTALL_CHANNEL", "dev", 1)
        defer {
            restoreEnvironmentValue(previousInstallChannel, forKey: "ALAN_INSTALL_CHANNEL")
        }

        let state = ShellStateSnapshot.bootstrapDefault()
        let pane = state.panes[0]
        let profile = AlanShellBootProfile.forPane(pane, shellState: state)

        expect(
            profile.environment["ALAN_INSTALL_CHANNEL"] == "dev",
            "dev boot profile must propagate ALAN_INSTALL_CHANNEL to child processes"
        )
    }

    private static func verifiesChannelScopedShellControlPaths() {
        let previousNamespace = ProcessInfo.processInfo.environment["ALAN_SHELL_CONTROL_NAMESPACE"]
        unsetenv("ALAN_SHELL_CONTROL_NAMESPACE")
        defer {
            restoreEnvironmentValue(previousNamespace, forKey: "ALAN_SHELL_CONTROL_NAMESPACE")
        }

        let stableRoot = alanShellControlPlaneRootURL(
            windowID: "window_main",
            channel: .stable
        )
        let devRoot = alanShellControlPlaneRootURL(
            windowID: "window_main",
            channel: .dev
        )
        let stableSocket = alanShellControlPlaneSocketURL(
            windowID: "window_main",
            channel: .stable
        )
        let devSocket = alanShellControlPlaneSocketURL(
            windowID: "window_main",
            channel: .dev
        )
        let stableBinding = alanShellBindingFileURL(
            windowID: "window_main",
            paneID: "pane_1",
            channel: .stable
        )
        let devBinding = alanShellBindingFileURL(
            windowID: "window_main",
            paneID: "pane_1",
            channel: .dev
        )

        expect(stableRoot != devRoot, "stable and dev shell-control roots must differ")
        expect(stableSocket != devSocket, "stable and dev shell-control sockets must differ")
        expect(stableBinding != devBinding, "stable and dev binding files must differ")
        expect(
            stableRoot.path.contains("/alan-shell-control/"),
            "stable shell-control root must use the stable namespace"
        )
        expect(
            devRoot.path.contains("/alan-dev-shell-control/"),
            "dev shell-control root must use the dev namespace"
        )
    }

    private static func verifiesDevBootProfileUsesDevShellControlNamespace() {
        let previousInstallChannel = ProcessInfo.processInfo.environment["ALAN_INSTALL_CHANNEL"]
        let previousNamespace = ProcessInfo.processInfo.environment["ALAN_SHELL_CONTROL_NAMESPACE"]
        setenv("ALAN_INSTALL_CHANNEL", "dev", 1)
        unsetenv("ALAN_SHELL_CONTROL_NAMESPACE")
        defer {
            restoreEnvironmentValue(previousInstallChannel, forKey: "ALAN_INSTALL_CHANNEL")
            restoreEnvironmentValue(previousNamespace, forKey: "ALAN_SHELL_CONTROL_NAMESPACE")
        }

        let state = ShellStateSnapshot.bootstrapDefault()
        let pane = state.panes[0]
        let profile = AlanShellBootProfile.forPane(pane, shellState: state)

        expect(
            profile.environment["ALAN_SHELL_CONTROL_DIR"]?.contains("/alan-dev-shell-control/") == true,
            "dev boot profile must expose the dev shell-control directory"
        )
        expect(
            profile.environment["ALAN_SHELL_SOCKET"]?.contains("/alan-dev-shell-control/") == true,
            "dev boot profile must expose the dev shell-control socket"
        )
        expect(
            profile.environment["ALAN_SHELL_BINDING_FILE"]?.contains("/alan-dev-shell-control/") == true,
            "dev boot profile must expose the dev binding file"
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

    private static func verifiesContentScopedDeliveryPreservesPendingDiagnostics() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_terminal_delivery"
        let handle = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_left",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle

        let accepted = service.sendText(toTerminalContentID: contentID, text: "hello")
        expect(accepted.applied, "content-keyed delivery must report accepted delivery")
        expect(accepted.acceptedBytes == 5, "content-keyed delivery must report accepted bytes")
        expect(handle.deliveredText == ["hello"], "content-keyed delivery must reach the surface")
        expect(
            service.snapshot(forTerminalContentID: contentID)?.lastDelivery == accepted,
            "content-keyed delivery diagnostics must stay on the terminal content snapshot"
        )

        _ = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_right",
            bootProfile: nil
        )
        let queuedText = "queued input"
        handle.deliveryResult = .queued(
            byteCount: queuedText.lengthOfBytes(using: .utf8),
            runtimePhase: "attachable"
        )

        let queued = service.sendText(toTerminalContentID: contentID, text: queuedText)
        expect(queued.code == .queued, "queued delivery must stay content keyed after remount")
        expect(
            queued.acceptedBytes == queuedText.lengthOfBytes(using: .utf8),
            "queued delivery must preserve queued byte count"
        )
        expect(queued.runtimePhase == "attachable", "queued delivery must preserve runtime phase")
        expect(
            service.snapshot(forTerminalContentID: contentID)?.paneID == "pane_right",
            "queued diagnostics must project the latest PaneSlot mount"
        )
        expect(
            service.snapshot(forTerminalContentID: contentID)?.lastDelivery == queued,
            "pending delivery state must remain observable by content ID"
        )

        let missing = service.sendText(toTerminalContentID: "content_missing", text: "hello")
        expect(missing.code == .missingTarget, "missing content must report runtime-missing")
        expect(missing.applied == false, "missing content delivery must not report applied")
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

    private static func verifiesExitedRuntimeTakesPrecedenceOverUnavailableRuntime() {
        let service = FakeAlanTerminalRuntimeService()
        let handle = service.surfaceHandle(for: "pane_1", bootProfile: nil) as! FakeAlanTerminalSurfaceHandle
        handle.ready = false
        handle.markProcessExited(exitCode: 0)

        let rejected = service.sendText(to: "pane_1", text: "after exit")

        expect(rejected.code == .rejected, "exited and unready runtime must report rejected")
        expect(
            rejected.errorCode == "terminal_child_exited",
            "exited runtime must take precedence over unavailable runtime"
        )
        expect(handle.deliveredText.isEmpty, "exited and unready runtime delivery must not reach the surface")
    }

    private static func verifiesUnavailableRuntimeDoesNotDeliverTextAndRecordsSnapshot() {
        let service = FakeAlanTerminalRuntimeService()
        let handle = service.surfaceHandle(for: "pane_1", bootProfile: nil) as! FakeAlanTerminalSurfaceHandle
        handle.ready = false

        let unavailable = service.sendText(to: "pane_1", text: "while unavailable")

        expect(unavailable.code == .unavailableRuntime, "unready runtime delivery must report unavailable")
        expect(unavailable.applied == false, "unavailable runtime delivery must not report applied")
        expect(
            unavailable.errorCode == "terminal_runtime_unavailable",
            "unavailable runtime delivery must use stable error code"
        )
        expect(handle.deliveredText.isEmpty, "unavailable runtime delivery must not reach the surface")
        expect(
            service.snapshot(for: "pane_1")?.lastDelivery == unavailable,
            "unavailable runtime delivery must stay observable in the metadata snapshot"
        )
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

    private static func verifiesRuntimeServiceCapturesLiveTranscriptSnapshot() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_capture_live"
        let handle = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_capture",
            bootProfile: sampleBootProfile(workingDirectory: "/repo/app")
        ) as! FakeAlanTerminalSurfaceHandle
        let range = AlanTerminalBufferRange(lowerBound: 0, upperBound: 2)
        handle.commandOutputTextByRange[range] = "build started\nbuild passed"
        handle.updateHostRuntimeSnapshot(
            transcriptRuntimeSnapshot(
                contentID: contentID,
                paneID: "pane_capture",
                cwd: "/repo/app",
                title: "make test",
                totalRows: 2,
                visibleRows: 2,
                firstVisibleRow: 0,
                mode: .normalBuffer
            )
        )

        let result = service.captureTranscriptSnapshot(forTerminalContentID: contentID)
        guard case .captured(let snapshot) = result else {
            fail("live terminal transcript capture must succeed")
        }
        expect(snapshot.contentID == contentID, "captured snapshot must be keyed by content identity")
        expect(snapshot.cwd == "/repo/app", "captured snapshot must preserve cwd")
        expect(snapshot.title == "make test", "captured snapshot must preserve terminal title")
        expect(snapshot.transcriptLines == ["build started", "build passed"], "capture must preserve text lines")
        expect(snapshot.dimensions?.rows == 2, "capture must preserve visible row dimensions")
        expect(snapshot.viewport?.firstVisibleRow == 0, "capture must preserve viewport anchor")
        expect(snapshot.alternateScreen == false, "normal-buffer capture must not report alternate screen")
    }

    private static func verifiesRuntimeServiceUsesRingBufferFallbackAndReportsFailures() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_capture_fallback"
        let handle = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_capture",
            bootProfile: sampleBootProfile(workingDirectory: "/repo/fallback")
        ) as! FakeAlanTerminalSurfaceHandle
        handle.recordTranscriptOutput("fallback one\nfallback two")
        handle.updateHostRuntimeSnapshot(
            transcriptRuntimeSnapshot(
                contentID: contentID,
                paneID: "pane_capture",
                cwd: "/repo/fallback",
                title: "zsh",
                totalRows: 2,
                visibleRows: 2,
                firstVisibleRow: 0,
                mode: .alternateScreen
            )
        )

        let fallback = service.captureTranscriptSnapshot(forTerminalContentID: contentID)
        guard case .captured(let snapshot) = fallback else {
            fail("runtime transcript capture must fall back to service-owned ring buffer")
        }
        expect(snapshot.transcriptLines == ["fallback one", "fallback two"], "fallback must preserve ring-buffer text")
        expect(snapshot.alternateScreen == true, "fallback capture must retain alternate-screen metadata")

        let missing = service.captureTranscriptSnapshot(forTerminalContentID: "content_missing")
        guard case .failed(let failure) = missing else {
            fail("missing runtime transcript capture must report explicit failure")
        }
        expect(failure.contentID == "content_missing", "capture failure must preserve requested content id")
        expect(failure.code == .missingRuntime, "missing runtime must use stable failure code")
    }

    private static func verifiesRuntimeServiceSeedsRestoredTranscriptBeforeInput() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_seeded"
        let snapshot = TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: "/repo/restored",
            title: "restored shell",
            dimensions: TerminalTranscriptDimensions(columns: 80, rows: 24),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 0, cursorRow: 1),
            transcriptLines: ["previous output"],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "inactive",
                program: "zsh",
                argvPreview: nil,
                lastCommandExitCode: 0
            ),
            capturedAt: Date(timeIntervalSince1970: 120),
            alternateScreen: false
        )

        service.seedRestoredTranscriptSnapshot(snapshot, forTerminalContentID: contentID)
        let handle = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_seeded",
            bootProfile: sampleBootProfile(workingDirectory: "/repo/restored")
        ) as! FakeAlanTerminalSurfaceHandle

        expect(
            handle.seededTranscriptSnapshot?.transcriptLines == ["previous output"],
            "surface handle must receive restored transcript before input"
        )
        let delivery = service.sendText(toTerminalContentID: contentID, text: "echo fresh\n")
        expect(delivery.applied, "seeded restored runtime must still accept fresh shell input")
        expect(handle.deliveredText == ["echo fresh\n"], "fresh input must route to the seeded runtime")
    }

    private static func verifiesRuntimeServiceClearsRestoredTranscriptCache() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_clear_restored"
        let snapshot = TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: "/repo/restored",
            title: "restored shell",
            dimensions: TerminalTranscriptDimensions(columns: 80, rows: 24),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 0, cursorRow: 1),
            transcriptLines: ["previous output"],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "inactive",
                program: "zsh",
                argvPreview: nil,
                lastCommandExitCode: 0
            ),
            capturedAt: Date(timeIntervalSince1970: 125),
            alternateScreen: false
        )

        service.seedRestoredTranscriptSnapshot(snapshot, forTerminalContentID: contentID)
        let first = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_clear_restored",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle
        expect(
            first.seededTranscriptSnapshot?.transcriptLines == ["previous output"],
            "test setup must seed the restored transcript"
        )

        service.clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
        expect(
            first.seededTranscriptSnapshot == nil,
            "clearing restored transcript cache must remove the snapshot from the mounted handle"
        )
        let delivery = service.sendText(toTerminalContentID: contentID, text: "echo fresh\n")
        expect(delivery.applied, "clearing the restored transcript must not tear down the live runtime")

        let remounted = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_clear_restored_again",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle
        expect(remounted === first, "clearing restored transcript must not recreate the live runtime")
        expect(
            remounted.seededTranscriptSnapshot == nil,
            "remounting cleared restored content must not reseed the old transcript"
        )
    }

    private static func verifiesFinalizeEvictsRestoredTranscriptCache() {
        let service = FakeAlanTerminalRuntimeService()
        let contentID = "content_reused"
        let snapshot = TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: "/repo/old",
            title: "old shell",
            dimensions: TerminalTranscriptDimensions(columns: 80, rows: 24),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 0, cursorRow: 1),
            transcriptLines: ["old restored output"],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "inactive",
                program: "zsh",
                argvPreview: nil,
                lastCommandExitCode: 0
            ),
            capturedAt: Date(timeIntervalSince1970: 140),
            alternateScreen: false
        )

        service.seedRestoredTranscriptSnapshot(snapshot, forTerminalContentID: contentID)
        let first = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_reused",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle
        expect(
            first.seededTranscriptSnapshot?.transcriptLines == ["old restored output"],
            "restored terminal must receive the cached transcript"
        )

        expect(
            service.finalizeTerminalContent(contentID) == .completed,
            "finalizing restored content must tear down the live handle"
        )
        let second = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_reused",
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle

        expect(second !== first, "reused content id must create a new handle after finalize")
        expect(
            second.seededTranscriptSnapshot == nil,
            "new terminal must not inherit a finalized restored transcript"
        )
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

    private static func sampleRuntimeSnapshot(
        priority: TerminalRuntimeRenderPriority,
        metadata: TerminalPaneMetadataSnapshot,
        renderer: TerminalRendererSnapshot = .placeholder,
        lastUpdatedAt: Date
    ) -> TerminalHostRuntimeSnapshot {
        TerminalHostRuntimeSnapshot(
            stage: .windowAttached,
            contentID: "content_terminal_policy",
            paneID: "pane_policy",
            tabID: "tab_policy",
            renderPriority: priority,
            logicalSize: .zero,
            backingSize: .zero,
            displayName: nil,
            displayID: nil,
            attachedWindowTitle: nil,
            isFocused: priority == .foregroundInteractive,
            renderer: renderer,
            paneMetadata: metadata,
            surfaceState: .placeholder,
            lastUpdatedAt: lastUpdatedAt
        )
    }

    private static func transcriptRuntimeSnapshot(
        contentID: String,
        paneID: String,
        cwd: String,
        title: String,
        totalRows: Int,
        visibleRows: Int,
        firstVisibleRow: Int,
        mode: AlanTerminalMode
    ) -> TerminalHostRuntimeSnapshot {
        let surfaceState = AlanTerminalSurfaceStateSnapshot(
            readiness: .ready,
            terminalMode: mode,
            scrollback: AlanTerminalScrollbackState(
                metrics: AlanTerminalScrollbackMetrics(
                    totalRows: totalRows,
                    visibleRows: visibleRows,
                    firstVisibleRow: firstVisibleRow,
                    mode: mode
                ),
                nativeScrollbarVisible: totalRows > visibleRows,
                thumbRange: firstVisibleRow..<max(firstVisibleRow, firstVisibleRow + visibleRows)
            ),
            search: nil,
            semanticCommands: .placeholder,
            readonly: false,
            secureInput: false,
            inputReady: true,
            rendererHealth: "ready",
            childExited: false,
            lastUpdatedAt: Date(timeIntervalSince1970: 120)
        )
        return TerminalHostRuntimeSnapshot(
            stage: .windowAttached,
            contentID: contentID,
            paneID: paneID,
            tabID: "tab_capture",
            renderPriority: .foregroundInteractive,
            logicalSize: CGSize(width: 80, height: visibleRows),
            backingSize: CGSize(width: 80, height: visibleRows),
            displayName: nil,
            displayID: nil,
            attachedWindowTitle: title,
            isFocused: true,
            renderer: .placeholder,
            paneMetadata: TerminalPaneMetadataSnapshot(
                title: title,
                workingDirectory: cwd,
                summary: nil,
                attention: .active,
                processExited: false,
                lastCommandExitCode: nil,
                lastUpdatedAt: Date(timeIntervalSince1970: 120),
                activeTaskState: .foregroundCommand
            ),
            surfaceState: surfaceState,
            lastUpdatedAt: Date(timeIntervalSince1970: 120)
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

    private static func expect(
        _ condition: @autoclosure () -> Bool,
        _ message: String
    ) {
        guard condition() else {
            fputs("error: \(message)\n", stderr)
            exit(1)
        }
    }

    private static func fail(_ message: String) -> Never {
        fputs("error: \(message)\n", stderr)
        exit(1)
    }

    private static func restoreEnvironmentValue(_ value: String?, forKey key: String) {
        if let value {
            setenv(key, value, 1)
        } else {
            unsetenv(key)
        }
    }
}

private final class FakeRenderCoordinatorHost: TerminalRenderCoordinatedHost {
    let id: String
    var priority: TerminalRuntimeRenderPriority
    var isRenderCoordinatorTargetAlive = true
    private(set) var appTickCount = 0
    private(set) var surfaceRefreshCount = 0
    private let recordEvent: (String) -> Void

    init(
        id: String,
        priority: TerminalRuntimeRenderPriority,
        events: @escaping (String) -> Void
    ) {
        self.id = id
        self.priority = priority
        self.recordEvent = events
    }

    var terminalRenderPriority: TerminalRuntimeRenderPriority {
        priority
    }

    func renderCoordinatorDrainAppTick() {
        appTickCount += 1
        recordEvent("tick:\(id)")
    }

    func renderCoordinatorRefreshSurface(reason: TerminalRenderRefreshReason) {
        surfaceRefreshCount += 1
        recordEvent("refresh:\(id):\(reason.rawValue)")
    }
}
#endif
