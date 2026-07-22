import Combine
import CoreGraphics
import Darwin
import Foundation

#if os(macOS)
import AppKit

@main
struct ShellRuntimeMetadataTestRunner {
    static func main() async {
        await MainActor.run {
            ShellRuntimeMetadataTests.run()
        }
    }
}

@MainActor
private enum ShellRuntimeMetadataTests {
    static func run() {
        // Runs first so the shared shell-core adapter is still uncached and can be forced into a
        // load failure via a missing dylib path.
        verifiesShellResolverFallsBackToLoginShellWhenCoreUnavailable()
        verifiesBootProfileCacheMemoizesPerLaunchKey()
        verifiesMetadataCallbackReusesCachedBootProfile()
        verifiesTerminalCallbackPathDoesNotWriteManifestSynchronously()
        verifiesTerminalCallbackBurstCoalescesIntoOnePersistenceWrite()
        verifiesLifecycleFlushPersistsPendingContentSynchronously()
        verifiesFailedStructuralManifestSaveIsReported()
        verifiesAsyncPersistenceWriteFailureRoutesToDiagnostics()
        verifiesPaneSupportDirectoryIsCreatedPromptlyAtBoot()
        verifiesSelectedRuntimeAssignmentIgnoresTimestampOnlyChanges()
        verifiesRuntimeProjectsTerminalStatusIntoPaneMetadata()
        verifiesRuntimeProjectionRecordsPerformanceDiagnostics()
        verifiesShellSelectionAndFocusRecordPerformanceDiagnostics()
        verifiesPerformanceDiagnosticsControlCommandsExportRecentBundle()
        verifiesTerminalContentProjectionAdapterOwnsMetadataProjection()
        verifiesSurfaceExitClosesFinalPaneWithoutRestarting()
        verifiesTerminalStatusSummaryPrioritizesExitAndRendererHealth()
        verifiesPaneTitleBarPrefersTerminalTitle()
        verifiesPaneTitleBarFallbackOrdering()
        verifiesPaneTitleBarSuppressesInternalTitles()
        verifiesOpeningTabSkipsStaleRuntimePaneIDs()
        verifiesRuntimeRegistryKeepsContentIdentityAcrossPaneMounts()
        verifiesRuntimeRegistryPreservesInteractiveBehaviorAcrossPaneRemount()
        verifiesRuntimeRegistryCleanupUsesCurrentMountContentIDs()
        verifiesRuntimeRegistryRekeysHostViewAcrossContentReplacement()
        verifiesRuntimeRegistryDeliversDeferredFocusAfterHostRegistration()
        verifiesTerminalLifecycleFinalizesClosedPaneAndPreservesSiblings()
        verifiesTerminalLifecycleFinalizesAllTabContentsOnce()
        verifiesTerminalLifecyclePreservesMovedAndLiftedRuntimes()
        verifiesMovingLastPaneClosesSourceTabWithoutFinalizingMovedRuntime()
        verifiesWorkspaceLifecycleRetirementFinalizesTerminalContentAndRejectsDelivery()
        verifiesTerminalLifecycleShutdownFinalizesAllRuntimes()
        verifiesShellHostControllerRoutesSharedAutomationCommands()
        verifiesControlPlaneRoutesSharedAutomationCommandSemantics()
        verifiesPortableControlDefaultsAndHostMetadata()
        verifiesSocketAndInProcessPortableCommandsShareExecutorSemantics()
        verifiesHostPaneSplitHonorsPaneSlotIdAndLaunchFields()
        verifiesControlPlaneTabOpenInheritsFocusedRuntimeCwd()
        verifiesSpaceCreateAllowsMoreThanNineSpacesAcrossCommandPaths()
        verifiesOpeningTerminalTabInheritsFocusedRuntimeCwd()
        verifiesShellActionNewTerminalTabInheritsFocusedRuntimeCwd()
        verifiesShellActionNewTerminalTabHonorsContextSpace()
        verifiesOpeningTerminalTabFallsBackToFocusedPaneSnapshotCwd()
        verifiesOpeningTerminalTabHonorsExplicitCwd()
        verifiesPortableZoomStateDoesNotChangePersistedSnapshotContract()
        verifiesSplitZoomLeavesCanonicalTreeAndKeepsSiblingRuntimes()
        verifiesTerminalRenderPrioritiesFollowSelectionAndZoom()
        verifiesTerminalRenderPrioritiesTrackWindowVisibility()
        verifiesSplitZoomIsTabScopedAndPrunedWhenPaneDisappears()
        verifiesInTabPaneMovementPreservesRuntimeContinuity()
        verifiesPaneMovementDragPolicyProtectsTerminalSelection()
        verifiesTerminalCommandResolverRoutesFocusedTerminalCommands()
        verifiesTerminalCommandResolverRejectsNonTerminalContent()
        verifiesCopyPasteRouteToFocusedTerminalRuntime()
        verifiesContextMenuTerminalCommandsUseContextPane()
        verifiesTerminalSearchRoutesThroughFocusedHostSurface()
        verifiesAdvancedControlPlaneResizeEqualizeAndEvents()
        verifiesSplitRatioEventsUseAffectedPaneForBackgroundTabs()
        verifiesAdvancedControlPlaneZoomFocusAndMovementResults()
        verifiesAdvancedControlPlaneRejectsUnknownUnzoomPane()
        verifiesShellCoreUnavailableFallbackIsReadOnly()
        verifiesPaneResizeSocketRequestsRequireHostHandler()
        verifiesPaneMoveSocketRequestsUseSharedExecutor()
        verifiesTerminalSendTextSocketRequestsRequireHostHandler()
        verifiesTerminalSendKeySocketRequestsRequireHostHandler()
        verifiesTerminalActivityProjectsByPaneID()
        verifiesProgressActivityFactoryUsesSourceFirstDisplay()
        verifiesCommandCompletionActivityFactory()
        verifiesTerminalActivityCodableUsesSnakeCase()
        verifiesTerminalActivitySidebarPriority()
        verifiesStaleProgressIsNotSidebarWorthy()
        verifiesDefaultSidebarActivitySelectionHonorsFreshness()
        verifiesSuccessfulCommandIsNotSidebarWorthy()
        verifiesCodexAgentActivityAdapterMapsSupportedStates()
        verifiesAgentActivityAdapterSanitizesDefaultUIPayload()
        verifiesAgentActivityAdapterRejectsMalformedPayloadAndFallsBackForUnsupportedAgent()
        verifiesAgentActivityControlCommandProjectsOntoPane()
        verifiesClearingActivityRemovesPaneActivity()
        verifiesPublishedStateMergeClearsActivity()
        verifiesPublishedStateMergeClearsTerminalProfileMetadata()
        verifiesPublishedStateMergePreservesSpaceSelection()
        verifiesPublishedStateMergePreservesContentContainers()
        verifiesPaneRebuildMutationsPreserveActivity()
        verifiesTabSidebarActivityProjectionUsesHighestPriorityPane()
        verifiesTabSidebarProjectionFallsBackToRepositoryBranch()
        verifiesTabSidebarProjectionPreservesTerminalStatusBeforeContext()
        verifiesTabSidebarProjectionDoesNotResurrectStaleCommandFailure()
        verifiesSidebarProgressRailBelongsToDisplayedActivity()
        verifiesFocusedCommandFailureDemotesFromSidebarProjection()
        verifiesCommandFailureAcknowledgementSticksAfterFocus()
        verifiesCommandFailureAcknowledgementRequiresFocusedPaneInTab()
        verifiesSpatialFocusAcknowledgesCommandFailure()
        verifiesActivityFreshnessPolicies()
        verifiesActivityAttentionIsReadTimeOnly()
        verifiesQuietAttentionNeverRequiresUserAction()
        verifiesPaneTitleActivityAccessoryLabel()
        verifiesPaneTitleDetailProjectionIncludesContextBranchAndProcess()
        verifiesPaneTitleDetailProjectionPreservesResponsivePriority()
        verifiesPaneTitleDetailProjectionAvoidsDuplicateAgentAndAlan()
        verifiesActivityNotificationPolicyIsLowNoise()
        verifiesControllerRoutesActivityNotificationsOnce()
        verifiesControllerRoutesDistinctActivityPayloadsInSameSecond()
        verifiesInactiveAppRoutesFocusedPaneNotifications()
        verifiesProcessExitNotificationRoutesBeforeAutoClose()
        verifiesProcessExitRuntimeNotificationRoutesBeforeAutoClose()
        verifiesTerminalChildExitIgnoresStaleForegroundContext()
        verifiesRuntimeExitCloseBypassesStaleActiveGuard()
        verifiesTerminalChildExitClosesSplitPane()
        verifiesTerminalChildExitClosesSinglePaneTab()
        verifiesTerminalChildExitCanLeaveEmptyFocusedSpace()
        verifiesClosingTabReleasesTerminalRuntime()
        verifiesActivePaneCloseRequiresConfirmationWithoutMutation()
        verifiesIdlePaneCloseBypassesCloseGuard()
        verifiesActiveTabCloseRequiresOneConfirmationForMultiplePanes()
        verifiesInteractivePaneCloseCancelLeavesStateManifestAndRuntimeUnchanged()
        verifiesInteractiveConfirmedPaneCloseCapturesSnapshotBeforeFinalization()
        verifiesConfirmedAppCloseRequestsGracefulShutdownBeforeTranscriptCapture()
        verifiesConfirmedAppCloseCapturesLatestTranscriptAfterGracefulTimeout()
        verifiesWindowAndAppCloseCancelRequireOneConfirmationWithoutMutation()
        verifiesControlPlaneClosePaneReportsRequiresConfirmation()
        verifiesControlPlaneCloseTabReportsRequiresConfirmation()
        verifiesControlPlaneCloseIdlePaneSucceeds()
        verifiesTabSelectionCommitsAuthoritativeFocus()
        verifiesShellActionTabNavigationTargetsCurrentSelection()
        verifiesSpaceSelectionCommitsAuthoritativeFocus()
        verifiesSpaceSelectionRestoresRememberedTab()
        verifiesShellActionSpaceSelectionReportsMissingTargets()
        verifiesSplitTabSelectionUsesStablePaneWithoutChangingLayout()
        verifiesWorkspaceManifestRestoresInactiveSpaceSelection()
        verifiesContentStateProjectionSeparatesPaneSlotsAndContent()
        verifiesContentRenderingRegistryRoutesSupportedKinds()
        verifiesContentAwareSidebarProjectionUsesNonTerminalLabels()
        verifiesOpeningContentTabDefaultsToTerminalIntent()
        verifiesOpeningMarkdownTabCreatesReadOnlyContentDescriptor()
        verifiesOpeningSettingsTabCreatesSingletonShellContent()
        verifiesSplitPaneAcceptsMarkdownContentIntent()
        verifiesControlPlaneResponsesExposeContentContainers()
        verifiesControlPlaneSendTextPreservesExplicitTerminalContentIdentity()
        verifiesControlPlanePropagatesRuntimeDeliveryFailures()
        verifiesControlFilePollerHandlesMalformedCommandFiles()
        verifiesControlFilePollerReportsResultWriteDiagnostics()
        verifiesContentContainerEventsCaptureLifecycleAndRejections()
        verifiesMixedContentPaneSlotMutationsStayContentAgnostic()
        verifiesChannelScopedSupportStatePaths()
        verifiesSmokeEnvironmentPathOverrides()
        verifiesWorkspaceManifestStartupRestoresPinnedSnapshot()
        verifiesWorkspaceManifestStartupSeedsRestoredTerminalTranscript()
        verifiesRestoredTranscriptPanelPresentationMatchesTerminalLayout()
        verifiesRestoredTranscriptDismissalClearsStateCacheAndManifest()
        verifiesTerminalClearActionDismissesRestoredTranscriptAndForwardsClear()
        verifiesClosingLastTabLeavesSelectedSpaceEmptyAndPersistsManifest()
        verifiesExplicitSpaceDeletionRemovesManifestSpace()
        verifiesPinSnapshotIsExplicitAndDoesNotTrackTransientChanges()
        verifiesMixedContentPinAndLiveSnapshotsPersistContentPayloads()
        verifiesOldManifestDecodesWithoutTerminalTranscriptSnapshot()
        verifiesTerminalTranscriptSnapshotsAreBoundedThroughManifestRoundTrip()
        verifiesPinnedRestoreOverlaysMatchingTranscriptWithoutMutatingTemplate()
        verifiesWorkspaceManifestSyncCapturesLiveTerminalTranscript()
        verifiesTabOrganizationPersistsOrderPinAndSpaceOwnership()
        verifiesManifestActiveTaskProjection()
        verifiesTerminalProfileStoreFallbackValidationAndCorruptRecovery()
        verifiesTerminalProfileLaunchResolutionAndEnvironmentProjection()
        verifiesTerminalProfileRebindingPreservesSpaceIconMetadata()
        verifiesTerminalProfileReferencesPersistThroughManifestRoundTrip()
        verifiesTerminalProfileInheritanceForSpacesTabsAndSplits()
        verifiesLegacyDefaultTerminalProfileDoesNotCaptureUnboundStartup()
        verifiesTerminalProfileControlPlaneOverrides()
        verifiesTerminalProfileSettingsRowsStayLocal()
        verifiesManagedTerminalAccountHelperPlanAndRollbackSafety()
        print("Shell runtime metadata tests passed.")
    }

    private static func verifiesRuntimeProjectsTerminalStatusIntoPaneMetadata() {
        let controller = makeController()
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: pane.terminalContentID,
                paneID: pane.paneID,
                tabID: pane.tabID,
                logicalSize: .zero,
                backingSize: .zero,
                displayName: "Studio Display",
                displayID: "display_1",
                attachedWindowTitle: "alan",
                isFocused: false,
                renderer: TerminalRendererSnapshot(
                    kind: .ghosttyLive,
                    phase: .failed,
                    summary: "renderer failed",
                    detail: "lost drawable",
                    failureReason: "lost device",
                    recentEvents: ["device lost"]
                ),
                paneMetadata: TerminalPaneMetadataSnapshot(
                    title: "vim main.rs",
                    workingDirectory: "/Users/morris/Developer/Alan",
                    summary: "terminal bell",
                    attention: .notable,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(timeIntervalSince1970: 1_000)
                ),
                surfaceState: AlanTerminalSurfaceStateSnapshot(
                    readiness: .unready(reason: .rendererFailed),
                    terminalMode: .normalBuffer,
                    scrollback: .empty,
                    search: nil,
                    semanticCommands: .placeholder,
                    readonly: false,
                    secureInput: false,
                    inputReady: false,
                    rendererHealth: "failed",
                    childExited: false,
                    lastUpdatedAt: Date(timeIntervalSince1970: 1_001)
                ),
                lastUpdatedAt: Date(timeIntervalSince1970: 1_002)
            )
        )

        let updated = controller.shellState.panes.first { $0.paneID == pane.paneID }
        expect(updated?.context?.rendererHealth == "failed", "pane context must record renderer health")
        expect(updated?.context?.surfaceReadiness == "renderer_failed", "pane context must record surface readiness")
        expect(updated?.context?.inputReady == false, "pane context must record input readiness")
        expect(updated?.context?.terminalMode == "normal_buffer", "pane context must record terminal mode")
        expect(updated?.viewport?.title == "vim main.rs", "pane viewport must record terminal title")
        expect(updated?.viewport?.summary == "Renderer failed", "pane viewport must expose renderer status")
        expect(updated?.attention == .notable, "pane attention must reflect terminal attention")
        expect(controller.shellState.spaces.first?.attention == .notable, "space attention must track pane attention")
    }

    private static func verifiesRuntimeProjectionRecordsPerformanceDiagnostics() {
        let recorder = AlanPerformanceDiagnosticsRecorder(
            configuration: AlanPerformanceDiagnosticsConfiguration(maxEvents: 16)
        )
        recorder.setEnabled(true)
        let controller = makeController(performanceDiagnosticsRecorder: recorder)
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: pane.terminalContentID,
                paneID: pane.paneID,
                tabID: pane.tabID,
                renderPriority: .foregroundInteractive,
                logicalSize: .zero,
                backingSize: .zero,
                displayName: "Studio Display",
                displayID: "display_1",
                attachedWindowTitle: "alan",
                isFocused: false,
                renderer: .placeholder,
                paneMetadata: TerminalPaneMetadataSnapshot(
                    title: "cargo test",
                    workingDirectory: "/repo/app",
                    summary: "build running",
                    attention: .active,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(timeIntervalSince1970: 2)
                ),
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: 3)
            )
        )

        let diagnostics = recorder.eventsSnapshot()
        expect(
            diagnostics.contains { $0.kind == .runtimeSnapshotPublish },
            "runtime update must record snapshot publication diagnostics"
        )
        expect(
            diagnostics.contains { $0.kind == .shellRuntimeProjection },
            "runtime update must record shell projection diagnostics"
        )
        expect(
            diagnostics.contains { $0.kind == .shellPaneStatePublication },
            "runtime projection must record pane-state publication diagnostics"
        )
        expect(
            diagnostics.contains {
                $0.paneID == pane.paneID
                    && $0.contentID == pane.terminalContentID
                    && $0.priority == "foregroundInteractive"
                    && $0.visibility == "visible"
            },
            "shell diagnostics must include pane/content correlation, priority, and visibility"
        )

        controller.updateTerminalMetadata(
            TerminalPaneMetadataSnapshot(
                title: "cargo test --workspace",
                workingDirectory: "/repo/app",
                summary: "metadata update",
                attention: .active,
                processExited: false,
                lastCommandExitCode: nil,
                lastUpdatedAt: Date(timeIntervalSince1970: 4)
            ),
            for: pane.paneID
        )
        expect(
            recorder.eventsSnapshot().contains { $0.kind == .terminalMetadataCallback },
            "terminal metadata updates must record metadata callback diagnostics"
        )
    }

    private static func verifiesShellSelectionAndFocusRecordPerformanceDiagnostics() {
        let recorder = AlanPerformanceDiagnosticsRecorder(
            configuration: AlanPerformanceDiagnosticsConfiguration(maxEvents: 32)
        )
        recorder.setEnabled(true)
        let controller = makeController(performanceDiagnosticsRecorder: recorder)
        _ = controller.openTerminalTab()
        recorder.setEnabled(false)
        recorder.setEnabled(true)

        controller.focus(paneID: "pane_1")
        controller.select(tabID: "tab_2")

        let events = recorder.eventsSnapshot()
        expect(
            events.contains { $0.kind == .shellFocusChange && $0.paneID == "pane_1" },
            "explicit focus changes must record performance diagnostics"
        )
        expect(
            events.contains { $0.kind == .shellSelectionChange && $0.paneID == "pane_2" },
            "tab selection must record selection diagnostics for the selected pane"
        )
        expect(
            events.contains { $0.kind == .shellPrioritySynchronization },
            "selection changes must record terminal priority synchronization diagnostics"
        )
        expect(
            events.contains {
                $0.kind == .shellSelectionChange
                    && $0.contentID == ShellContentInstance.terminalContentID(forPaneID: "pane_2")
                    && $0.visibility == "visible"
            },
            "selection diagnostics must keep pane/content correlation without terminal text"
        )
    }

    private static func verifiesPerformanceDiagnosticsControlCommandsExportRecentBundle() {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("alan-control-diagnostics-\(UUID().uuidString)", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        } catch {
            fail("diagnostics control command test must create temp dir: \(error)")
        }
        defer {
            try? FileManager.default.removeItem(at: root)
            AlanPerformanceDiagnosticsController.shared.setEnabled(false)
        }

        AlanPerformanceDiagnosticsController.shared.setEnabled(false)
        let controller = makeController()
        let enableResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "diagnostics-enable",
                  "command": "performance_diagnostics.set_enabled",
                  "enabled": true
                }
                """
            )
        )
        expect(enableResponse.applied == true, "diagnostics enable control command must apply")
        expect(enableResponse.diagnosticsEnabled == true, "diagnostics enable response must report enabled")

        let childPressureResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "diagnostics-child-pressure",
                  "command": "performance_diagnostics.record_child_pressure",
                  "child_process_role": "terminal_child",
                  "child_cpu_percent": 181.5,
                  "child_memory_bytes": 4096,
                  "child_thread_count": 7
                }
                """
            )
        )
        expect(childPressureResponse.applied == true,
               "diagnostics child-pressure control command must apply")

        guard let pane = controller.selectedPane else {
            fail("diagnostics control command test must expose a selected pane")
        }
        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: pane.terminalContentID,
                paneID: pane.paneID,
                tabID: pane.tabID,
                renderPriority: .foregroundInteractive,
                logicalSize: .zero,
                backingSize: .zero,
                displayName: "Studio Display",
                displayID: "display_1",
                attachedWindowTitle: "alan",
                isFocused: true,
                renderer: .placeholder,
                paneMetadata: .placeholder,
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: 5)
            )
        )

        let exportResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "diagnostics-export",
                  "command": "performance_diagnostics.export_recent",
                  "export_directory": "\(jsonEscaped(root.path))"
                }
                """
            )
        )
        expect(exportResponse.applied == true, "diagnostics export control command must apply")
        guard let bundlePath = exportResponse.diagnosticsBundlePath else {
            fail("diagnostics export response must include bundle path")
        }
        let bundle = URL(fileURLWithPath: bundlePath, isDirectory: true)
        let events = try? String(
            contentsOf: bundle.appendingPathComponent("events.jsonl"),
            encoding: .utf8
        )
        let summary = try? String(
            contentsOf: bundle.appendingPathComponent("summary.json"),
            encoding: .utf8
        )
        expect(events?.contains("shellRuntimeProjection") == true,
               "exported diagnostics must include shell projection events")
        expect(summary?.contains("\"schemaVersion\"") == true,
               "exported diagnostics summary must include schema metadata")
        expect(summary?.contains("\"terminalChild\"") == true,
               "exported diagnostics must include terminal child CPU pressure")
        expect(events?.contains(pane.paneID) == false,
               "exported diagnostics must not include raw pane ids")
        expect(events?.contains("pane_id_hash") == true,
               "exported diagnostics must keep hashed pane correlation")

        let disableResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "diagnostics-disable",
                  "command": "performance_diagnostics.set_enabled",
                  "enabled": false
                }
                """
            )
        )
        expect(disableResponse.applied == true, "diagnostics disable control command must apply")
        expect(disableResponse.diagnosticsEnabled == false, "diagnostics disable response must report disabled")
        expect(
            AlanPerformanceDiagnosticsController.shared.eventsSnapshot().isEmpty,
            "diagnostics disable control command must clear retained in-memory events"
        )
    }

    private static func verifiesTerminalContentProjectionAdapterOwnsMetadataProjection() {
        let controller = makeController()
        guard let pane = controller.selectedPane,
              let bootProfile = controller.bootProfile(for: pane)
        else {
            fail("bootstrap shell must expose a selected pane and boot profile")
        }

        let adapter = TerminalContentProjectionAdapter(
            paneProjection: ShellPaneProjectionService()
        )
        let metadata = TerminalPaneMetadataSnapshot(
            title: "vim main.rs",
            workingDirectory: "/tmp/alan",
            summary: "build running",
            attention: .active,
            processExited: false,
            lastCommandExitCode: 0,
            lastUpdatedAt: Date(timeIntervalSince1970: 4_000),
            activeTaskState: .foregroundCommand
        )
        let runtime = TerminalHostRuntimeSnapshot(
            stage: .windowAttached,
            contentID: pane.terminalContentID,
            paneID: pane.paneID,
            tabID: pane.tabID,
            logicalSize: .zero,
            backingSize: .zero,
            displayName: "Studio Display",
            displayID: "display_1",
            attachedWindowTitle: "alan",
            isFocused: false,
            renderer: TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .surfaceReady,
                summary: "surface ready",
                detail: nil,
                failureReason: nil,
                recentEvents: []
            ),
            paneMetadata: metadata,
            surfaceState: AlanTerminalSurfaceStateSnapshot(
                readiness: .ready,
                terminalMode: .normalBuffer,
                scrollback: .empty,
                search: nil,
                semanticCommands: .placeholder,
                readonly: false,
                secureInput: false,
                inputReady: true,
                rendererHealth: "ready",
                childExited: false,
                lastUpdatedAt: Date(timeIntervalSince1970: 4_001)
            ),
            lastUpdatedAt: Date(timeIntervalSince1970: 4_002)
        )

        let projection = adapter.projectRuntime(runtime, for: pane, bootProfile: bootProfile)

        expect(projection.pane.cwd == "/tmp/alan", "terminal adapter must project runtime cwd")
        expect(projection.pane.viewport?.title == "vim main.rs", "terminal adapter must project title")
        expect(projection.pane.viewport?.summary == "build running", "terminal adapter must project summary")
        expect(
            projection.pane.context?.processState == "foreground_command",
            "terminal adapter must project process state"
        )
        expect(
            projection.pane.context?.surfaceReadiness == "ready",
            "terminal adapter must project surface readiness"
        )
        expect(projection.pane.context?.inputReady == true, "terminal adapter must project input readiness")
        expect(projection.pane.attention == .active, "terminal adapter must project attention")

        let profiledPane = ShellPane(
            paneID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            launchTarget: pane.launchTarget,
            cwd: pane.cwd,
            process: pane.process,
            attention: pane.attention,
            context: pane.context,
            viewport: pane.viewport,
            activity: pane.activity,
            alanBinding: pane.alanBinding,
            terminalProfileID: "alan"
        )
        let profiledRuntimeProjection = adapter.projectRuntime(
            runtime,
            for: profiledPane,
            bootProfile: bootProfile
        )
        expect(
            profiledRuntimeProjection.pane.terminalProfileID == "alan",
            "terminal adapter runtime projection must preserve captured terminal profile id"
        )
        let profiledDefaultCwdPane = ShellPane(
            paneID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            launchTarget: pane.launchTarget,
            cwd: nil,
            process: pane.process,
            attention: pane.attention,
            context: pane.context,
            viewport: pane.viewport,
            activity: pane.activity,
            alanBinding: pane.alanBinding,
            terminalProfileID: "alan"
        )
        let profiledDefaultCwdRuntimeProjection = adapter.projectRuntime(
            runtime,
            for: profiledDefaultCwdPane,
            bootProfile: bootProfile
        )
        expect(
            profiledDefaultCwdRuntimeProjection.pane.cwd == nil,
            "terminal adapter runtime projection must preserve nil cwd for profile-bound panes"
        )
        let profiledBootProjection = adapter.projectBootContext(
            runtime: runtime,
            for: profiledPane,
            bootProfile: bootProfile
        )
        expect(
            profiledBootProjection.pane.terminalProfileID == "alan",
            "terminal adapter boot projection must preserve captured terminal profile id"
        )
        let profiledDefaultCwdBootProjection = adapter.projectBootContext(
            runtime: runtime,
            for: profiledDefaultCwdPane,
            bootProfile: bootProfile
        )
        expect(
            profiledDefaultCwdBootProjection.pane.cwd == nil,
            "terminal adapter boot projection must preserve nil cwd for profile-bound panes"
        )

        let binding = ShellAlanBinding(
            processPath: "/proc/1",
            machineState: "waiting",
            pendingRequest: true,
            source: "test",
            lastProjectedAt: "2026-05-22T00:00:00Z"
        )
        let bindingProjection = adapter.projectAlanBinding(
            binding,
            runtime: runtime,
            for: projection.pane,
            bootProfile: bootProfile
        )

        expect(bindingProjection.pane.alanBinding == binding, "terminal adapter must project alan binding")
        expect(
            bindingProjection.pane.attention == .awaitingUser,
            "terminal adapter must project pending-yield attention"
        )
        let profiledBindingProjection = adapter.projectAlanBinding(
            binding,
            runtime: runtime,
            for: profiledPane,
            bootProfile: bootProfile
        )
        expect(
            profiledBindingProjection.pane.terminalProfileID == "alan",
            "terminal adapter binding projection must preserve captured terminal profile id"
        )
        let profiledDefaultCwdBindingProjection = adapter.projectAlanBinding(
            binding,
            runtime: runtime,
            for: profiledDefaultCwdPane,
            bootProfile: bootProfile
        )
        expect(
            profiledDefaultCwdBindingProjection.pane.cwd == nil,
            "terminal adapter binding projection must preserve nil cwd for profile-bound panes"
        )
        expect(
            bindingProjection.pane.viewport?.summary == "alan is waiting for user input",
            "terminal adapter must project alan binding summary"
        )
        expect(
            bindingProjection.pane.viewport?.lastActivityAt == "2026-05-22T00:00:00Z",
            "terminal adapter must project binding activity time"
        )
    }

    private static func verifiesSurfaceExitClosesFinalPaneWithoutRestarting() {
        let controller = makeController()
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: pane.terminalContentID,
                paneID: pane.paneID,
                tabID: pane.tabID,
                logicalSize: .zero,
                backingSize: .zero,
                displayName: "Studio Display",
                displayID: "display_1",
                attachedWindowTitle: "alan",
                isFocused: false,
                renderer: TerminalRendererSnapshot(
                    kind: .ghosttyLive,
                    phase: .surfaceReady,
                    summary: "surface ready",
                    detail: nil,
                    failureReason: nil,
                    recentEvents: []
                ),
                paneMetadata: TerminalPaneMetadataSnapshot(
                    title: "fish",
                    workingDirectory: "/Users/morris/Developer/Alan",
                    summary: "terminal rendering",
                    attention: .idle,
                    processExited: false,
                    lastCommandExitCode: 7,
                    lastUpdatedAt: Date(timeIntervalSince1970: 2_000)
                ),
                surfaceState: AlanTerminalSurfaceStateSnapshot(
                    readiness: .unready(reason: .childExited),
                    terminalMode: .normalBuffer,
                    scrollback: .empty,
                    search: nil,
                    semanticCommands: .placeholder,
                    readonly: false,
                    secureInput: false,
                    inputReady: false,
                    rendererHealth: "surface_ready",
                    childExited: true,
                    lastUpdatedAt: Date(timeIntervalSince1970: 2_001)
                ),
                lastUpdatedAt: Date(timeIntervalSince1970: 2_002)
            )
        )

        expect(
            controller.shellState.pane(paneID: pane.paneID) == nil,
            "surface child exit must close the owning final pane"
        )
        expect(
            controller.shellState.spaces.first?.tabs.isEmpty == true,
            "surface child exit must leave the focused space empty instead of restarting a terminal"
        )
        expect(controller.shellState.focusedPaneID == nil, "surface child exit must clear pane focus")
    }

    private static func verifiesTerminalStatusSummaryPrioritizesExitAndRendererHealth() {
        let exited = pane(
            context: context(
                processState: "exited",
                rendererHealth: "ready",
                surfaceReadiness: "child_exited",
                lastCommandExitCode: 2
            ),
            viewport: ShellViewportSnapshot(
                title: "fish",
                summary: "terminal bell",
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            attention: .awaitingUser
        )
        expect(shellTerminalStatusSummary(for: exited) == "Exited 2", "exit status must outrank cwd or generic summaries")

        let failedRenderer = pane(
            context: context(
                processState: "running",
                rendererHealth: "failed",
                surfaceReadiness: "renderer_failed",
                lastCommandExitCode: nil
            ),
            viewport: ShellViewportSnapshot(
                title: "fish",
                summary: "terminal bell",
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            attention: .notable
        )
        expect(shellTerminalStatusSummary(for: failedRenderer) == "Renderer failed", "renderer failure must outrank generic summaries")

        let ordinary = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: ShellViewportSnapshot(
                title: "fish",
                summary: "idle shell",
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            attention: .idle
        )
        expect(shellTerminalStatusSummary(for: ordinary) == nil, "ordinary summaries must not hide cwd or branch metadata")
    }

    private static func verifiesPaneTitleBarPrefersTerminalTitle() {
        let title = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: "alan",
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: "vim main.rs - fish",
                    summary: nil,
                    visibleExcerpt: nil,
                    lastActivityAt: nil
                ),
                cwd: "/Users/morris/Developer/Alan",
                process: ShellProcessBinding(program: "fish", argvPreview: nil),
                attention: .idle
            )
        )

        expect(title == "vim main.rs", "pane title bar must prefer normalized terminal title over cwd")
    }

    private static func verifiesPaneTitleBarFallbackOrdering() {
        let cwdTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: "Workspace",
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: nil,
                cwd: "/tmp/project",
                process: ShellProcessBinding(program: "fish", argvPreview: nil),
                attention: .idle
            )
        )
        expect(cwdTitle == "project", "pane title bar must use cwd leaf before working-directory name")

        let workingDirectoryTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: "Workspace",
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: nil,
                cwd: nil,
                process: ShellProcessBinding(program: "fish", argvPreview: nil),
                attention: .idle
            )
        )
        expect(workingDirectoryTitle == "Workspace", "pane title bar must use working directory when cwd is missing")

        let alanProcessTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: nil,
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: nil,
                cwd: nil,
                process: ShellProcessBinding(program: "alan", argvPreview: nil),
                attention: .idle
            )
        )
        expect(
            alanProcessTitle == "alan",
            "pane title bar must still expose user-launched alan processes"
        )

        let processTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: nil,
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: nil,
                cwd: nil,
                process: ShellProcessBinding(program: "fish", argvPreview: nil),
                attention: .idle
            )
        )
        expect(processTitle == "fish", "pane title bar must use process fallback before generic Terminal")
    }

    private static func verifiesPaneTitleBarSuppressesInternalTitles() {
        let debugTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: "alan",
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: "title updated",
                    summary: nil,
                    visibleExcerpt: nil,
                    lastActivityAt: nil
                ),
                cwd: "/Users/morris/Developer/Alan",
                process: ShellProcessBinding(program: "fish", argvPreview: nil),
                attention: .idle
            )
        )
        expect(debugTitle == "alan", "pane title bar must suppress debug title text")

        let rawPaneTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: "Workspace",
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: "pane_42",
                    summary: nil,
                    visibleExcerpt: nil,
                    lastActivityAt: nil
                ),
                cwd: nil,
                process: ShellProcessBinding(program: "fish", argvPreview: nil),
                attention: .idle
            )
        )
        expect(rawPaneTitle == "Workspace", "pane title bar must suppress raw pane IDs")

        let longTitle = "ssh production-shell-with-a-very-long-title.example.com"
        let preservedLongTitle = shellPaneTitleBarTitle(
            for: pane(
                context: context(
                    workingDirectoryName: nil,
                    processState: "running",
                    rendererHealth: "ready",
                    surfaceReadiness: "ready",
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: longTitle,
                    summary: nil,
                    visibleExcerpt: nil,
                    lastActivityAt: nil
                ),
                cwd: nil,
                process: nil,
                attention: .idle
            )
        )
        expect(preservedLongTitle == longTitle, "pane title helper must leave long titles available for UI truncation")
    }

    private static func verifiesOpeningTabSkipsStaleRuntimePaneIDs() {
        let windowID = "metadata_test_\(UUID().uuidString)"
        let registry = TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        let context = ShellWindowContext.make(
            windowID: windowID,
            terminalRuntimeRegistry: registry
        )
        let controller = ShellHostController(
            shellState: .bootstrapDefault(windowID: windowID),
            windowContext: context,
            terminalRuntimeRegistry: registry
        )
        let stalePane = ShellPane(
            paneID: "pane_2",
            tabID: "tab_stale",
            spaceID: "space_main",
            launchTarget: .shell,
            cwd: "/tmp",
            process: nil,
            attention: .idle,
            context: nil,
            viewport: nil,
            alanBinding: nil
        )
        _ = registry.surfaceHandle(for: stalePane, bootProfile: nil)

        expect(
            registry.registeredPaneIDs.contains("pane_2"),
            "test setup must register a runtime-only stale pane"
        )

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "runtime-reserved-tab-open-1",
                  "command": "tab.open"
                }
                """
            )
        )

        expect(
            response.applied == true && controller.selectedPane?.paneID == "pane_3",
            "shared tab.open must skip pane IDs still owned by the terminal runtime registry"
        )
        expect(
            !registry.registeredPaneIDs.contains("pane_2"),
            "opening a tab must release stale runtime-only panes after adopting the new state"
        )
    }

    private static func verifiesRuntimeRegistryKeepsContentIdentityAcrossPaneMounts() {
        let registry = TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        let contentID = "content_terminal_runtime_primary"
        let firstMount = TerminalContentMount(
            contentID: contentID,
            paneSlotID: "pane_left",
            tabID: "tab_1",
            spaceID: "space_main"
        )
        let secondMount = TerminalContentMount(
            contentID: contentID,
            paneSlotID: "pane_right",
            tabID: "tab_2",
            spaceID: "space_main"
        )

        let first = registry.surfaceHandle(forTerminalContent: firstMount, bootProfile: nil)
        let second = registry.surfaceHandle(forTerminalContent: secondMount, bootProfile: nil)

        expect(first === second, "registry must reuse runtime handles by terminal content identity")
        expect(second.contentID == contentID, "registry handle must retain content identity")
        expect(second.paneID == "pane_right", "registry handle must project the latest PaneSlot mount")
        expect(registry.registeredContentIDs == [contentID], "registry registration must be content keyed")
        expect(registry.registeredPaneIDs == ["pane_right"], "registry pane IDs must reflect current mounts")

        registry.updateSnapshot(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: contentID,
                paneID: "pane_right",
                tabID: "tab_2",
                logicalSize: .zero,
                backingSize: .zero,
                displayName: nil,
                displayID: nil,
                attachedWindowTitle: nil,
                isFocused: false,
                renderer: .placeholder,
                paneMetadata: .placeholder,
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: 10)
            )
        )
        expect(
            registry.snapshot(forTerminalContentID: contentID).paneID == "pane_right",
            "registry snapshot lookup must be content keyed"
        )

        let delivery = registry.sendText(to: "pane_right", text: "after remount")
        let handle = second as! FakeAlanTerminalSurfaceHandle
        expect(delivery.applied, "pane convenience delivery must resolve the mounted content ID")
        expect(handle.deliveredText == ["after remount"], "delivery must reach the mounted content")

        registry.releaseRuntimes(excluding: ["pane_right"])
        expect(handle.teardownCount == 0, "active custom content ID must not be released as stale")
        expect(
            registry.registeredContentIDs == [contentID],
            "cleanup must keep the active mounted content registered"
        )

        registry.releaseRuntime(for: "pane_right")
        expect(handle.teardownCount == 1, "pane convenience release must finalize the mounted content")
        expect(registry.registeredContentIDs.isEmpty, "released content must leave the registry")
    }

    private static func verifiesRuntimeRegistryPreservesInteractiveBehaviorAcrossPaneRemount() {
        let runtimeService = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: runtimeService)
        let contentID = "content_terminal_interactive"
        let firstMount = TerminalContentMount(
            contentID: contentID,
            paneSlotID: "pane_left",
            tabID: "tab_1",
            spaceID: "space_main"
        )
        let secondMount = TerminalContentMount(
            contentID: contentID,
            paneSlotID: "pane_right",
            tabID: "tab_2",
            spaceID: "space_main"
        )
        let firstHostView = registry.hostView(
            forTerminalContent: firstMount,
            pane: nil,
            bootProfile: nil,
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )
        let handle = registry.surfaceHandle(
            forTerminalContent: firstMount,
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle

        let replacementHostView = AlanTerminalHostNSView()
        registry.configureHostView(
            replacementHostView,
            forTerminalContent: secondMount,
            pane: nil,
            bootProfile: nil,
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )
        let remountedHandle = registry.surfaceHandle(
            forTerminalContent: secondMount,
            bootProfile: nil
        )

        expect(remountedHandle === handle, "reattachment must keep the content-keyed runtime handle")
        expect(firstHostView.teardownCount == 0, "view reattachment must not teardown the runtime")
        expect(handle.paneID == "pane_right", "remounted handle must project the latest PaneSlot")
        expect(
            registry.terminalCommandRuntimeState(for: "pane_right").inputReady,
            "terminal input readiness must follow the remounted terminal content"
        )
        expect(
            !registry.terminalCommandRuntimeState(for: "pane_left").inputReady,
            "stale PaneSlot must not remain an input target after remount"
        )

        handle.selectedText = "remounted selection"
        let pasteboard = RecordingTerminalPasteboardWriter()
        expect(
            registry.copySelection(for: "pane_right", to: pasteboard),
            "copy must resolve through the remounted terminal content"
        )
        expect(
            pasteboard.string == "remounted selection",
            "copy must read selection from the content-keyed runtime"
        )

        expect(
            registry.beginFindInteraction(for: "pane_right"),
            "search must resolve through the remounted terminal host"
        )
        expect(
            handle.searchActions == ["start_search"],
            "search must reach the remounted content runtime"
        )

        let paste = registry.pasteText("paste after remount", to: "pane_right")
        expect(paste.applied, "paste must deliver through the remounted terminal content")
        expect(
            handle.deliveredText.last == "paste after remount",
            "paste must reach the content-keyed runtime"
        )

        let queuedText = "queued after remount"
        handle.deliveryResult = .queued(
            byteCount: queuedText.lengthOfBytes(using: .utf8),
            runtimePhase: "attachable"
        )
        let queued = registry.sendText(to: "pane_right", text: queuedText)
        expect(queued.code == .queued, "terminal text delivery must preserve queued state")
        expect(
            runtimeService.snapshot(forTerminalContentID: contentID)?.lastDelivery == queued,
            "pending delivery diagnostics must stay on the remounted content"
        )

        let staleDelivery = registry.sendText(to: "pane_left", text: "stale target")
        expect(
            staleDelivery.code == .missingTarget,
            "stale PaneSlot delivery must not fall through to the remounted runtime"
        )
    }

    private static func verifiesRuntimeRegistryCleanupUsesCurrentMountContentIDs() {
        let registry = TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        let previousMount = TerminalContentMount(
            contentID: "content_previous_terminal",
            paneSlotID: "pane_stable",
            tabID: "tab_1",
            spaceID: "space_main"
        )
        let currentMount = TerminalContentMount(
            contentID: "content_current_terminal",
            paneSlotID: "pane_stable",
            tabID: "tab_1",
            spaceID: "space_main"
        )

        let previousHandle = registry.surfaceHandle(
            forTerminalContent: previousMount,
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle

        registry.releaseRuntimes(excluding: [currentMount])
        expect(
            previousHandle.teardownCount == 1,
            "cleanup must release stale content before the current host remount is configured"
        )
        expect(
            registry.registeredContentIDs.isEmpty,
            "current mount registration must not keep the previous content alive"
        )

        let currentHandle = registry.surfaceHandle(
            forTerminalContent: currentMount,
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle
        registry.releaseRuntimes(excluding: [currentMount])
        expect(
            currentHandle.teardownCount == 0,
            "cleanup must keep the currently mounted content alive"
        )
    }

    private static func verifiesRuntimeRegistryRekeysHostViewAcrossContentReplacement() {
        let registry = TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        let firstMount = TerminalContentMount(
            contentID: "content_terminal_first",
            paneSlotID: "pane_stable",
            tabID: "tab_1",
            spaceID: "space_main"
        )
        let secondMount = TerminalContentMount(
            contentID: "content_terminal_second",
            paneSlotID: "pane_stable",
            tabID: "tab_1",
            spaceID: "space_main"
        )

        let hostView = registry.hostView(
            forTerminalContent: firstMount,
            pane: nil,
            bootProfile: nil,
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )
        let firstHandle = registry.surfaceHandle(
            forTerminalContent: firstMount,
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle

        registry.configureHostView(
            hostView,
            forTerminalContent: secondMount,
            pane: nil,
            bootProfile: nil,
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )

        let secondHandle = registry.surfaceHandle(
            forTerminalContent: secondMount,
            bootProfile: nil
        ) as! FakeAlanTerminalSurfaceHandle
        expect(
            registry.beginFindInteraction(for: "pane_stable"),
            "same-pane content replacement must re-register host actions to the new content"
        )
        expect(
            secondHandle.searchActions == ["start_search"],
            "host actions must reach the replacement content surface"
        )

        registry.requestFocus(for: "pane_stable")
        expect(hostView.focusCount == 1, "focus must route through the re-keyed host view")

        registry.releaseRuntimes(excluding: ["pane_stable"])
        expect(
            firstHandle.teardownCount == 1,
            "cleanup must release stale content after same-pane replacement"
        )
        expect(
            hostView.teardownCount == 0,
            "stale content cleanup must not teardown the re-keyed active host view"
        )
        expect(
            secondHandle.teardownCount == 0,
            "active replacement content must remain alive during stale cleanup"
        )

        registry.releaseRuntime(for: "pane_stable")
        expect(
            hostView.teardownCount == 1,
            "pane release must teardown the active re-keyed host view"
        )
        expect(
            secondHandle.teardownCount == 1,
            "pane release must finalize the active replacement content"
        )
    }

    private static func verifiesRuntimeRegistryDeliversDeferredFocusAfterHostRegistration() {
        let registry = TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        let delayedMount = TerminalContentMount(
            contentID: "content_delayed_focus",
            paneSlotID: "pane_delayed_focus",
            tabID: "tab_delayed_focus",
            spaceID: "space_main"
        )
        let hostView = AlanTerminalHostNSView()

        registry.requestFocus(for: "pane_delayed_focus", retryBudget: 3)
        expect(hostView.focusCount == 0, "early focus must wait for host view registration")

        registry.configureHostView(
            hostView,
            forTerminalContent: delayedMount,
            pane: nil,
            bootProfile: nil,
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )
        expect(
            hostView.focusCount == 1,
            "pending focus must be delivered exactly once when the host view registers"
        )

        registry.configureHostView(
            hostView,
            forTerminalContent: delayedMount,
            pane: nil,
            bootProfile: nil,
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )
        expect(
            hostView.focusCount == 1,
            "deferred focus must not continuously refocus after registration"
        )
    }

    private static func verifiesTerminalLifecycleFinalizesClosedPaneAndPreservesSiblings() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let leftHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let rightHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)

        _ = controller.closePane(paneID: "pane_2")

        expect(leftHandle.teardownCount == 0, "closing one pane must preserve sibling terminal runtime")
        expect(rightHandle.teardownCount == 1, "closing one pane must finalize its terminal runtime once")
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs == ["pane_1"],
            "closed PaneSlot must stop being a terminal runtime target"
        )
    }

    private static func verifiesTerminalLifecycleFinalizesAllTabContentsOnce() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let leftHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let rightHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)

        _ = controller.closeTab(tabID: "tab_main")
        _ = controller.closeTab(tabID: "tab_main")

        expect(leftHandle.teardownCount == 1, "closing a tab must finalize first terminal once")
        expect(rightHandle.teardownCount == 1, "closing a tab must finalize second terminal once")
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.isEmpty,
            "closed tab terminal runtimes must leave the registry"
        )
    }

    private static func verifiesActivePaneCloseRequiresConfirmationWithoutMutation() {
        let controller = makeController()
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "make test", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        let stateBefore = controller.snapshotJSON

        switch controller.closePane(paneID: "pane_1") {
        case .requiresConfirmation(let impact):
            expect(impact.activeTerminalContentIDs == ["content_pane_1"], "active pane close must report guarded content")
            expect(impact.scope == .paneSlot("pane_1"), "active pane close impact must preserve requested scope")
        case .closed, .paneNotFound, .lastTab:
            fail("active pane close must require confirmation")
        }

        expect(controller.snapshotJSON == stateBefore, "guarded pane close must not mutate shell state")
        expect(handle.teardownCount == 0, "guarded pane close must not finalize terminal runtime")
    }

    private static func verifiesIdlePaneCloseBypassesCloseGuard() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let handle = fakeSurfaceHandle(for: "pane_2", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "zsh", activeTaskState: .inactive),
            for: "pane_2"
        )

        switch controller.closePane(paneID: "pane_2") {
        case .closed:
            break
        case .requiresConfirmation, .paneNotFound, .lastTab:
            fail("idle pane close must bypass active-work confirmation")
        }

        expect(controller.pane(paneID: "pane_2") == nil, "idle close must remove the pane")
        expect(handle.teardownCount == 1, "idle close must finalize the closed terminal runtime")
    }

    private static func verifiesActiveTabCloseRequiresOneConfirmationForMultiplePanes() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let leftHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let rightHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "cargo test", activeTaskState: .foregroundCommand),
            for: "pane_2"
        )
        let stateBefore = controller.snapshotJSON

        switch controller.closeTab(tabID: "tab_main") {
        case .requiresConfirmation(let impact):
            expect(impact.scope == .tab("tab_main"), "tab close impact must preserve requested tab")
            expect(impact.affectedTerminalContentIDs.count == 2, "tab close impact must collect all terminal panes")
            expect(impact.activeTerminalContentIDs == ["content_pane_2"], "tab close must identify active terminal only once")
        case .closed, .tabNotFound, .lastTab:
            fail("active tab close must require one confirmation")
        }

        expect(controller.snapshotJSON == stateBefore, "guarded tab close must not mutate shell state")
        expect(leftHandle.teardownCount == 0, "guarded tab close must preserve idle sibling runtime")
        expect(rightHandle.teardownCount == 0, "guarded tab close must preserve active runtime")
    }

    private static func verifiesInteractivePaneCloseCancelLeavesStateManifestAndRuntimeUnchanged() {
        let windowID = "interactive_cancel_\(UUID().uuidString)"
        let manifestURL = manifestURL("interactive_cancel")
        let service = FakeAlanTerminalRuntimeService()
        let presenter = FakeShellCloseConfirmationPresenter(nextResponses: [false])
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let controller = makeController(
            windowID: windowID,
            terminalRuntimeRegistry: registry,
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: manifestURL),
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/repo/app",
                now: Date(timeIntervalSince1970: 80)
            ),
            closeConfirmationPresenter: presenter
        )
        _ = fakeSurfaceHandle(for: "pane_1", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "npm run dev", cwd: "/repo/app", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        let stateBeforeClose = controller.shellState
        let manifestBeforeClose = decodeManifest(at: manifestURL)

        expect(
            controller.requestClosePane(paneID: "pane_1") == false,
            "interactive active pane close must stop when confirmation is cancelled"
        )
        expect(presenter.impacts.count == 1, "interactive close must present one confirmation")
        expect(controller.shellState == stateBeforeClose, "cancelled close must leave shell state unchanged")
        expect(
            decodeManifest(at: manifestURL) == manifestBeforeClose,
            "cancelled close must leave workspace manifest unchanged"
        )
        expect(
            service.registeredContentIDs.contains("content_pane_1"),
            "cancelled close must leave terminal runtime alive"
        )
    }

    private static func verifiesInteractiveConfirmedPaneCloseCapturesSnapshotBeforeFinalization() {
        let service = FakeAlanTerminalRuntimeService()
        let presenter = FakeShellCloseConfirmationPresenter(nextResponses: [true])
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let controller = makeController(
            terminalRuntimeRegistry: registry,
            closeConfirmationPresenter: presenter,
            gracefulShutdownTimeout: 0
        )
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let range = AlanTerminalBufferRange(lowerBound: 0, upperBound: 1)
        handle.commandOutputTextByRange[range] = "build running"
        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: "content_pane_1",
                paneID: "pane_1",
                tabID: "tab_main",
                logicalSize: CGSize(width: 80, height: 1),
                backingSize: CGSize(width: 80, height: 1),
                displayName: nil,
                displayID: nil,
                attachedWindowTitle: "make build",
                isFocused: true,
                renderer: .placeholder,
                paneMetadata: metadata(
                    title: "make build",
                    cwd: "/repo/app",
                    activeTaskState: .foregroundCommand
                ),
                surfaceState: AlanTerminalSurfaceStateSnapshot(
                    readiness: .ready,
                    terminalMode: .normalBuffer,
                    scrollback: AlanTerminalScrollbackState(
                        metrics: AlanTerminalScrollbackMetrics(
                            totalRows: 1,
                            visibleRows: 1,
                            firstVisibleRow: 0,
                            mode: .normalBuffer
                        ),
                        nativeScrollbarVisible: false,
                        thumbRange: 0..<1
                    ),
                    search: nil,
                    semanticCommands: .placeholder,
                    readonly: false,
                    secureInput: false,
                    inputReady: true,
                    rendererHealth: "ready",
                    childExited: false,
                    lastUpdatedAt: Date(timeIntervalSince1970: 81)
                ),
                lastUpdatedAt: Date(timeIntervalSince1970: 81)
            )
        )

        expect(
            controller.requestClosePane(paneID: "pane_1"),
            "confirmed active pane close must apply"
        )
        expect(presenter.impacts.count == 1, "confirmed close must present one confirmation")
        expect(
            handle.captureTranscriptTextRanges == [range],
            "confirmed close must capture transcript before runtime finalization"
        )
        expect(handle.teardownCount == 1, "confirmed close must finalize the terminal runtime")
        expect(controller.pane(paneID: "pane_1") == nil, "confirmed close must remove the pane")
    }

    private static func verifiesConfirmedAppCloseRequestsGracefulShutdownBeforeTranscriptCapture() {
        let windowID = "graceful_app_close_\(UUID().uuidString)"
        let manifestURL = manifestURL("graceful_app_close")
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let presenter = FakeShellCloseConfirmationPresenter(nextResponses: [true])
        let controller = makeController(
            windowID: windowID,
            terminalRuntimeRegistry: registry,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/repo/app",
                now: Date(timeIntervalSince1970: 82)
            ),
            closeConfirmationPresenter: presenter,
            gracefulShutdownTimeout: 0
        )
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        handle.recordTranscriptOutput("codex running")
        handle.markActiveTaskState(.foregroundCommand)
        handle.onGracefulShutdownRequest = { reason in
            expect(reason == .appQuit, "app quit must request app-quit graceful shutdown")
            handle.recordTranscriptOutput("codex resume previous-session-id")
            handle.markActiveTaskState(.inactive)
            expect(
                !controller.closePaneAfterTerminalRuntimeExit(paneID: "pane_1"),
                "confirmed graceful close must suppress runtime-exit auto-close before transcript capture"
            )
            controller.updateTerminalMetadata(
                metadata(
                    title: "codex",
                    cwd: "/repo/app",
                    processExited: true,
                    activeTaskState: .inactive
                ),
                for: "pane_1"
            )
            expect(
                controller.pane(paneID: "pane_1") != nil,
                "confirmed graceful close must keep the pane available until transcript capture"
            )
        }
        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )

        expect(controller.requestTerminateApp(), "confirmed app quit must apply after graceful request")
        expect(
            handle.gracefulShutdownRequests == [.appQuit],
            "confirmed app quit must request graceful shutdown for active terminal content"
        )
        expect(
            handle.deliveredKeys == [.interrupt],
            "graceful shutdown must send terminal interrupt before finalization"
        )

        guard let savedManifest = decodeManifest(at: manifestURL),
              let liveSnapshot = savedManifest.spaces.first?.tabs.first?.liveSnapshot,
              let payload = terminalPayload(in: liveSnapshot, paneSlotID: "pane_1"),
              let transcript = payload.transcriptSnapshot
        else {
            fail("confirmed app quit must persist a transcript snapshot")
        }
        expect(
            transcript.transcriptLines == [
                "codex running",
                "codex resume previous-session-id",
            ],
            "transcript capture must include output printed during graceful shutdown"
        )
        expect(handle.teardownCount == 1, "confirmed app quit must still finalize the runtime")
    }

    private static func verifiesConfirmedAppCloseCapturesLatestTranscriptAfterGracefulTimeout() {
        let windowID = "graceful_timeout_\(UUID().uuidString)"
        let manifestURL = manifestURL("graceful_timeout")
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let presenter = FakeShellCloseConfirmationPresenter(nextResponses: [true])
        let controller = makeController(
            windowID: windowID,
            terminalRuntimeRegistry: registry,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/repo/app",
                now: Date(timeIntervalSince1970: 83)
            ),
            closeConfirmationPresenter: presenter,
            gracefulShutdownTimeout: 0.001
        )
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        handle.recordTranscriptOutput("long command still running")
        handle.markActiveTaskState(.foregroundCommand)
        controller.updateTerminalMetadata(
            metadata(title: "cargo test", cwd: "/repo/app", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )

        expect(controller.requestTerminateApp(), "confirmed app quit must not block forever on graceful timeout")
        expect(
            handle.deliveredKeys == [.interrupt],
            "timeout path must still request graceful shutdown once"
        )
        expect(handle.teardownCount == 1, "timeout path must force-finalize the runtime")
        expect(
            controller.controlPlaneDiagnostics.contains {
                $0.contains("terminal graceful shutdown timed out")
            },
            "timeout path must record a graceful shutdown diagnostic"
        )

        guard let savedManifest = decodeManifest(at: manifestURL),
              let liveSnapshot = savedManifest.spaces.first?.tabs.first?.liveSnapshot,
              let transcript = terminalPayload(
                in: liveSnapshot,
                paneSlotID: "pane_1"
              )?.transcriptSnapshot
        else {
            fail("timeout path must still persist latest transcript tail")
        }
        expect(
            transcript.transcriptLines == ["long command still running"],
            "timeout path must capture the latest available transcript tail"
        )
    }

    private static func verifiesWindowAndAppCloseCancelRequireOneConfirmationWithoutMutation() {
        let service = FakeAlanTerminalRuntimeService()
        let presenter = FakeShellCloseConfirmationPresenter(nextResponses: [false, false])
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let controller = makeController(
            terminalRuntimeRegistry: registry,
            closeConfirmationPresenter: presenter
        )
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "cargo test", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        let stateBeforeClose = controller.shellState

        expect(
            controller.requestCloseWindow() == false,
            "active window close must stop when confirmation is cancelled"
        )
        expect(controller.shellState == stateBeforeClose, "cancelled window close must preserve shell state")
        expect(handle.teardownCount == 0, "cancelled window close must preserve runtime")

        expect(
            controller.requestTerminateApp() == false,
            "active app quit must stop when confirmation is cancelled"
        )
        expect(
            presenter.impacts.map(\.scope) == [.window, .app],
            "window close and app quit must each present one scoped confirmation"
        )
        expect(controller.shellState == stateBeforeClose, "cancelled app quit must preserve shell state")
        expect(handle.teardownCount == 0, "cancelled app quit must preserve runtime")
    }



    private static func verifiesControlPlaneClosePaneReportsRequiresConfirmation() {
        let controller = makeController()
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "npm run dev", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        let stateBefore = controller.snapshotJSON

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "close-active-pane",
                  "command": "pane.close",
                  "pane_id": "pane_1"
                }
                """
            )
        )

        expect(response.applied == false, "control close must not apply when confirmation is required")
        expect(response.errorCode == "requires_confirmation", "control close must report stable confirmation code")
        expect(response.paneID == "pane_1", "control close response must identify guarded pane")
        expect(response.contentID == "content_pane_1", "control close response must identify guarded content")
        expect(controller.snapshotJSON == stateBefore, "control close rejection must not mutate shell state")
        expect(handle.teardownCount == 0, "control close rejection must not finalize runtime")
    }

    private static func verifiesControlPlaneCloseTabReportsRequiresConfirmation() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let leftHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let rightHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "cargo test", activeTaskState: .foregroundCommand),
            for: "pane_2"
        )
        let stateBefore = controller.snapshotJSON

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "close-active-tab",
                  "command": "tab.close",
                  "tab_id": "tab_main"
                }
                """
            )
        )

        expect(response.applied == false, "control tab close must not apply when confirmation is required")
        expect(response.errorCode == "requires_confirmation", "control tab close must report stable confirmation code")
        expect(response.tabID == "tab_main", "control tab close response must identify guarded tab")
        expect(controller.snapshotJSON == stateBefore, "control tab close rejection must not mutate shell state")
        expect(leftHandle.teardownCount == 0, "control tab close rejection must preserve idle sibling runtime")
        expect(rightHandle.teardownCount == 0, "control tab close rejection must preserve active runtime")
    }

    private static func verifiesControlPlaneCloseIdlePaneSucceeds() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let handle = fakeSurfaceHandle(for: "pane_2", controller: controller)
        controller.updateTerminalMetadata(
            metadata(title: "zsh", activeTaskState: .inactive),
            for: "pane_2"
        )

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "close-idle-pane",
                  "command": "pane.close",
                  "pane_id": "pane_2"
                }
                """
            )
        )

        expect(response.applied == true, "control idle pane close must preserve existing success semantics")
        expect(response.errorCode == nil, "control idle pane close must not report confirmation")
        expect(controller.pane(paneID: "pane_2") == nil, "control idle pane close must remove the pane")
        expect(handle.teardownCount == 1, "control idle pane close must finalize the runtime")
    }

    private static func verifiesTerminalLifecyclePreservesMovedAndLiftedRuntimes() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let movedHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let siblingHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)
        guard let targetTabID = controller.openTerminalTab() else {
            fail("test setup must create target tab")
        }

        expect(
            controller.movePane(paneID: "pane_1", toTab: targetTabID, direction: .horizontal),
            "pane move must apply"
        )
        expect(movedHandle.teardownCount == 0, "moving a pane must preserve moved runtime")
        expect(siblingHandle.teardownCount == 0, "moving a pane must preserve source sibling runtime")

        switch controller.liftPaneToTab(paneID: "pane_1") {
        case .lifted:
            break
        case .lastPane, .paneNotFound:
            fail("lift after move must apply while the target tab still has a sibling")
        }
        expect(movedHandle.teardownCount == 0, "lifting a pane must preserve moved runtime")
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.contains("pane_1"),
            "lifted PaneSlot must remain a terminal runtime target"
        )
    }

    private static func verifiesMovingLastPaneClosesSourceTabWithoutFinalizingMovedRuntime() {
        let controller = makeController()
        let movedHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        guard let targetTabID = controller.openTerminalTab() else {
            fail("test setup must create target tab")
        }

        expect(
            controller.movePane(paneID: "pane_1", toTab: targetTabID, direction: .vertical),
            "moving the last source pane must apply"
        )

        expect(controller.shellState.tab(tabID: "tab_main") == nil, "empty source tab must close")
        expect(movedHandle.teardownCount == 0, "source tab cleanup must not finalize moved runtime")
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.contains("pane_1"),
            "moved PaneSlot must remain a terminal runtime target"
        )
    }

    private static func verifiesWorkspaceLifecycleRetirementFinalizesTerminalContentAndRejectsDelivery() {
        let controller = makeController()
        let retainedHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        guard let retiredTabID = controller.openTerminalTab(title: "Retired") else {
            fail("retirement setup must create a terminal tab")
        }
        guard let retiredPaneID = controller.shellState.panes(in: retiredTabID).first?.paneID,
              let retiredContentID = controller.shellState
                .contentStateProjection()
                .contentMounted(in: retiredPaneID)?
                .contentID
        else {
            fail("retirement setup must expose retired terminal content")
        }
        let retiredHandle = fakeSurfaceHandle(for: retiredPaneID, controller: controller)

        do {
            let prunedResult = try controller.shellState.closingTab(retiredTabID)
            controller.applyMutationResult(prunedResult)
        } catch {
            fail("retirement setup must adopt a pruned shell state: \(error)")
        }

        expect(
            retainedHandle.teardownCount == 0,
            "workspace lifecycle retirement must preserve retained terminal runtimes"
        )
        expect(
            retiredHandle.teardownCount == 1,
            "workspace lifecycle retirement must finalize retired terminal content once"
        )
        expect(
            controller.terminalRuntimeRegistry.registeredContentIDs.contains(retiredContentID) == false,
            "retired terminal content must leave the runtime registry"
        )

        let delivery = controller.terminalRuntimeRegistry.sendText(
            toTerminalContentID: retiredContentID,
            text: "after retirement"
        )
        expect(
            delivery.applied == false && delivery.code == .missingTarget,
            "retired terminal content must not receive later runtime delivery"
        )

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "retired-terminal-send-1",
                  "command": "terminal.send_text",
                  "content_id": "\(retiredContentID)",
                  "text": "ignored"
                }
                """
            )
        )
        expect(
            response.applied == false && response.errorCode == "content_not_found",
            "retired terminal content must not remain a control-plane delivery target"
        )
    }

    private static func verifiesTerminalLifecycleShutdownFinalizesAllRuntimes() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let leftHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let rightHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)

        controller.shutdownTerminalRuntimes()
        controller.shutdownTerminalRuntimes()

        expect(leftHandle.teardownCount == 1, "shutdown must finalize first terminal once")
        expect(rightHandle.teardownCount == 1, "shutdown must finalize second terminal once")
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.isEmpty,
            "shutdown must clear terminal runtime registry state"
        )
    }

    private static func verifiesShellHostControllerRoutesSharedAutomationCommands() {
        let controller = makeController()
        let handler: ShellAutomationCommandHandling = controller

        let split = handler.performShellAutomationCommand(
            .splitPane(ShellAutomationPaneSplitRequest(paneID: "pane_1", placement: .right))
        )

        guard split.code == .accepted,
              let splitPaneID = split.paneID,
              controller.pane(paneID: splitPaneID) != nil
        else {
            fail("shared split command must create and return the new pane")
        }

        let summary = handler.performShellAutomationCommand(.readPaneSummary(paneID: splitPaneID))
        expect(
            summary.code == .accepted && summary.summary?.paneID == splitPaneID,
            "shared read-summary command must return safe pane metadata"
        )

        let focus = handler.performShellAutomationCommand(.focusPane(paneID: "pane_1"))
        expect(
            focus.code == .accepted && controller.shellState.focusedPaneID == "pane_1",
            "shared focus command must update controller focus"
        )

        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let send = handler.performShellAutomationCommand(
            .sendText(ShellAutomationSendTextRequest(paneID: "pane_1", text: "pwd\n"))
        )
        expect(
            send.code == .accepted && send.deliveryCode == "accepted" && send.acceptedBytes == 4,
            "shared send-text command must expose runtime delivery semantics"
        )
        expect(handle.deliveredText == ["pwd\n"], "shared send-text command must reach runtime")

        let close = handler.performShellAutomationCommand(.closePane(paneID: splitPaneID))
        expect(
            close.code == .accepted && controller.pane(paneID: splitPaneID) == nil,
            "shared close-pane command must close the target pane"
        )

        let missing = handler.performShellAutomationCommand(.focusPane(paneID: "missing"))
        expect(
            missing.code == .missingTarget && !missing.applied,
            "shared command handler must report missing targets with stable semantics"
        )
    }

    private static func verifiesHostPaneSplitHonorsPaneSlotIdAndLaunchFields() {
        let controller = makeController(
            windowID: "host_split_launch",
            shellState: .bootstrapDefault(
                windowID: "host_split_launch",
                workingDirectory: "/Users/morris/project"
            )
        )

        // Canonical `pane_slot_id` targeting plus explicit launch fields must both survive the
        // host-routed pane.split path.
        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "host-split-launch-1",
                  "command": "pane.split",
                  "pane_slot_id": "pane_1",
                  "direction": "horizontal",
                  "cwd": "/Users/morris/explicit",
                  "title": "Build Watcher"
                }
                """
            )
        )
        guard response.applied == true, let newPaneID = response.paneID else {
            fail("host pane.split must accept pane_slot_id and apply")
        }
        expect(
            controller.shellState.pane(paneID: newPaneID)?.cwd == "/Users/morris/explicit",
            "host pane.split must honor the explicit cwd instead of inheriting the source pane"
        )
        expect(
            controller.shellState
                .contentStateProjection()
                .contentMounted(in: newPaneID)?
                .title == "Build Watcher",
            "host pane.split must honor the explicit title instead of the generic one"
        )

        let inheritedResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "host-split-pane-slot-defaults-1",
                  "command": "pane.split",
                  "pane_slot_id": "pane_1",
                  "direction": "vertical"
                }
                """
            )
        )
        expect(
            inheritedResponse.paneID.flatMap { controller.shellState.pane(paneID: $0)?.cwd }
                == "/Users/morris/project",
            "pane_slot_id split defaults must inherit the source login-shell cwd"
        )

        guard let profileState = ShellStateSnapshot.bootstrapDefault(
            windowID: "host_split_profile_defaults",
            workingDirectory: "/Users/morris/project"
        ).settingTerminalProfile("profile-main", forSpaceID: "space_main")
        else {
            fail("profile split defaults test must bind the source Space profile")
        }
        let profileController = makeController(
            windowID: "host_split_profile_defaults",
            shellState: profileState
        )
        let profileResponse = profileController.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "host-split-pane-slot-profile-1",
                  "command": "pane.split",
                  "pane_slot_id": "pane_1",
                  "direction": "vertical"
                }
                """
            )
        )
        let profilePane = profileResponse.paneID.flatMap {
            profileController.shellState.pane(paneID: $0)
        }
        expect(
            profilePane?.terminalProfileID == "profile-main" && profilePane?.cwd == nil,
            "pane_slot_id split defaults must inherit the source Space profile"
        )
    }

    private static func verifiesControlPlaneTabOpenInheritsFocusedRuntimeCwd() {
        let controller = makeController()
        controller.updateTerminalMetadata(
            metadata(title: "cwd update", cwd: "/repo/control"),
            for: "pane_1"
        )

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "control-tab-inherit-cwd-1",
                  "command": "tab.open"
                }
                """
            )
        )

        expect(
            response.applied == true
                && response.paneID.flatMap { controller.shellState.pane(paneID: $0)?.cwd }
                    == "/repo/control",
            "shared tab.open must inherit the focused runtime cwd when no profile is selected"
        )
    }

    private static func verifiesControlPlaneRoutesSharedAutomationCommandSemantics() {
        let controller = makeController()

        let split = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "automation-split-1",
                  "command": "pane.split",
                  "pane_id": "pane_1",
                  "direction": "vertical"
                }
                """
            )
        )
        guard split.applied == true,
              let splitPaneID = split.paneID,
              controller.pane(paneID: splitPaneID) != nil
        else {
            fail("control-plane split must use accepted shared command semantics")
        }

        let missingFocus = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "automation-focus-missing-1",
                  "command": "pane.focus",
                  "pane_id": "missing"
                }
                """
            )
        )
        expect(
            missingFocus.applied == false && missingFocus.errorCode == "pane_not_found",
            "control-plane focus must report shared missing-target semantics"
        )

        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let send = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "automation-send-1",
                  "command": "terminal.send_text",
                  "pane_id": "pane_1",
                  "text": "pwd\\n"
                }
                """
            )
        )
        expect(
            send.applied == true && send.deliveryCode == "accepted" && send.acceptedBytes == 4,
            "control-plane send_text must expose shared runtime delivery semantics"
        )
        expect(
            handle.deliveredText == ["pwd\n"],
            "control-plane send_text must route through the shared runtime command path"
        )
    }

    private static func verifiesSocketAndInProcessPortableCommandsShareExecutorSemantics() {
        let fixture = makeController(windowID: "shared_control_fixture")
        _ = fixture.splitPane(paneID: "pane_1", placement: .right)
        let initialState = fixture.shellState
        let inProcessController = makeController(
            windowID: "shared_control_in_process",
            shellState: initialState
        )
        let command = decodeControlCommand(
            """
            {
              "request_id": "shared-control-focus-1",
              "command": "pane.focus",
              "pane_id": "pane_1"
            }
            """
        )
        let inProcessResponse = inProcessController.handleControlPlaneCommand(command)

        var socketAdoptedState: ShellStateSnapshot?
        let socketServer = AlanShellSocketServer(
            socketURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("shared-control-\(UUID().uuidString).sock"),
            commandHandler: { _ in fail("portable pane.focus must execute locally") },
            stateAdoptionHandler: { socketAdoptedState = $0 },
            sideEffectHandler: { _ in fail("pane.focus must not emit a platform side effect") }
        )
        _ = socketServer.mergePublishedState(initialState)
        let socketResponse = socketServer.handleLocally(command)

        expect(socketResponse != nil, "socket pane.focus must use the local command executor")
        expect(
            socketResponse?.applied == inProcessResponse.applied
                && socketResponse?.spaceID == inProcessResponse.spaceID
                && socketResponse?.tabID == inProcessResponse.tabID
                && socketResponse?.paneID == inProcessResponse.paneID
                && socketResponse?.errorCode == inProcessResponse.errorCode,
            "socket and in-process callers must expose the same portable response semantics"
        )
        expect(
            socketAdoptedState?.focusedPaneID == inProcessController.shellState.focusedPaneID,
            "socket and in-process callers must adopt the same portable state"
        )
    }

    private static func verifiesPortableControlDefaultsAndHostMetadata() {
        let runtimeService = FakeAlanTerminalRuntimeService()
        runtimeService.renderCoordinatorMetricsOverride = TerminalRenderCoordinatorMetrics()
        let controller = makeController(
            windowID: "portable_control_defaults",
            terminalRuntimeRegistry: TerminalRuntimeRegistry(runtimeService: runtimeService)
        )

        let stateResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "portable-state-metadata-1",
                  "command": "state"
                }
                """
            )
        )
        expect(
            stateResponse.terminalRenderMetrics != nil,
            "shared state response must retain host render metrics"
        )

        let selectedTabID = controller.selectedTabID
        let pinResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "portable-pin-default-1",
                  "command": "tab.pin"
                }
                """
            )
        )
        expect(
            pinResponse.applied == true
                && pinResponse.tabID == selectedTabID
                && pinResponse.sourceSpaceID == controller.shellState.focusedSpaceID,
            "shared tab.pin must retain selected-tab defaults and source projection"
        )
        _ = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "portable-unpin-default-1",
                  "command": "tab.unpin"
                }
                """
            )
        )

        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        controller.focus(paneID: "pane_1")
        let zoomResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "portable-zoom-default-1",
                  "command": "pane.zoom"
                }
                """
            )
        )
        expect(
            zoomResponse.applied == true
                && zoomResponse.paneID == "pane_1"
                && zoomResponse.latestEventID != nil,
            "shared pane.zoom must retain focused-pane defaults and host event metadata"
        )
        let unzoomResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "portable-unzoom-default-1",
                  "command": "pane.unzoom"
                }
                """
            )
        )
        expect(
            unzoomResponse.applied == true
                && unzoomResponse.paneID == "pane_1"
                && unzoomResponse.mountedContentInstanceID
                    == ShellContentInstance.terminalContentID(forPaneID: "pane_1")
                && unzoomResponse.latestEventID != nil,
            "shared pane.unzoom must report the prior zoom target and host event metadata"
        )
    }

    private static func verifiesSpaceCreateAllowsMoreThanNineSpacesAcrossCommandPaths() {
        var expandedState = ShellStateSnapshot.bootstrapDefault(windowID: "space_track_control")
        while expandedState.spaces.count < 10 {
            expandedState = expandedState.creatingTerminalSpace(
                title: nil,
                workingDirectory: nil
            ).state
        }

        let controller = makeController(
            windowID: "space_track_control",
            shellState: expandedState
        )
        let directSpaceID = controller.createSpace()
        expect(directSpaceID != nil, "direct createSpace must allow more than nine Spaces")
        expect(
            controller.shellState.spaces.count == 11,
            "direct createSpace must append the eleventh Space"
        )

        let controlPlaneCreate = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "space-track-control-plane",
                  "command": "space.create"
                }
                """
            )
        )
        expect(
            controlPlaneCreate.applied == true && controlPlaneCreate.spaceID != nil,
            "control-plane space.create must allow more than nine Spaces"
        )
        expect(
            controller.shellState.spaces.count == 12,
            "control-plane space.create must append another Space beyond the old cap"
        )

        let localCreate = AlanShellLocalCommandExecutor.execute(
            command: decodeControlCommand(
                """
                {
                  "request_id": "space-track-local",
                  "command": "space.create"
                }
                """
            ),
            state: expandedState
        )
        expect(
            localCreate?.response.applied == true && localCreate?.response.spaceID != nil,
            "local space.create must allow more than nine Spaces"
        )
        expect(
            localCreate?.updatedState?.spaces.count == 11,
            "local space.create must return an updated state beyond the old cap"
        )
    }

    private static func verifiesOpeningTerminalTabInheritsFocusedRuntimeCwd() {
        let controller = makeController()
        controller.updateTerminalMetadata(metadata(title: "cwd update", cwd: "/repo/app"), for: "pane_1")

        _ = controller.openTerminalTab()

        expect(
            controller.selectedPane?.cwd == "/repo/app",
            "new terminal tabs must inherit the focused pane runtime cwd"
        )
    }

    private static func verifiesShellActionNewTerminalTabInheritsFocusedRuntimeCwd() {
        let controller = makeController()
        controller.updateTerminalMetadata(metadata(title: "cwd update", cwd: "/repo/app"), for: "pane_1")

        let result = controller.performShellAction(.newTerminalTab)

        expect(result == .executed, "registry new-terminal action must execute")
        expect(
            controller.selectedPane?.cwd == "/repo/app",
            "registry new-terminal action must inherit the focused pane runtime cwd"
        )
    }

    private static func verifiesShellActionNewTerminalTabHonorsContextSpace() {
        let controller = makeController()
        let originalSpaceID = controller.selectedSpaceID
        guard let targetSpaceID = controller.createTerminalSpace(
            title: "Target",
            workingDirectory: "/target"
        ) else {
            fail("test setup must create a target Space")
        }
        if let originalSpaceID {
            controller.select(spaceID: originalSpaceID)
        }
        let originalTargetTabCount = controller.shellState.space(spaceID: targetSpaceID)?.tabs.count

        let result = controller.performShellAction(
            .newTerminalTab,
            target: .contextSpace(targetSpaceID),
            source: .contextMenu
        )

        expect(result == .executed, "sidebar new-terminal action must execute")
        expect(
            controller.shellState.space(spaceID: targetSpaceID)?.tabs.count == (originalTargetTabCount ?? 0) + 1,
            "sidebar new-terminal action must create the tab in the context Space"
        )
        expect(
            controller.selectedSpaceID == targetSpaceID,
            "sidebar new-terminal action must focus the context Space after creating its tab"
        )
    }

    private static func verifiesOpeningTerminalTabFallsBackToFocusedPaneSnapshotCwd() {
        let windowID = "metadata_snapshot_cwd_\(UUID().uuidString)"
        let controller = makeController(
            windowID: windowID,
            shellState: .bootstrapDefault(windowID: windowID, workingDirectory: "/snapshot/cwd")
        )

        _ = controller.openTerminalTab()

        expect(
            controller.selectedPane?.cwd == "/snapshot/cwd",
            "new terminal tabs must fall back to the focused pane snapshot cwd"
        )
    }

    private static func verifiesOpeningTerminalTabHonorsExplicitCwd() {
        let controller = makeController()
        controller.updateTerminalMetadata(metadata(title: "cwd update", cwd: "/repo/app"), for: "pane_1")

        _ = controller.openTerminalTab(workingDirectory: "/explicit/cwd")

        expect(
            controller.selectedPane?.cwd == "/explicit/cwd",
            "explicit new-tab cwd must override focused pane cwd"
        )
    }

    private static func verifiesSplitZoomLeavesCanonicalTreeAndKeepsSiblingRuntimes() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        guard let tab = controller.selectedTab else {
            fail("test setup must keep a selected tab")
        }
        let canonicalTree = tab.paneTree
        _ = controller.terminalRuntimeRegistry.surfaceHandle(
            for: controller.pane(paneID: "pane_1"),
            bootProfile: controller.bootProfile(for: controller.pane(paneID: "pane_1"))
        )
        _ = controller.terminalRuntimeRegistry.surfaceHandle(
            for: controller.pane(paneID: "pane_2"),
            bootProfile: controller.bootProfile(for: controller.pane(paneID: "pane_2"))
        )

        expect(controller.zoomPane(paneID: "pane_1"), "zoom must accept a split pane")
        expect(controller.selectedTabZoomedPaneID == "pane_1", "zoom state must be tab scoped")
        expect(
            controller.displayPaneTree(for: controller.selectedTab)?.paneIDs == ["pane_1"],
            "zoomed display tree must project only the zoomed pane"
        )
        expect(
            controller.selectedTab?.paneTree == canonicalTree,
            "zoom must leave the canonical split tree unchanged"
        )
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.isSuperset(of: ["pane_1", "pane_2"]),
            "zoom must not release sibling terminal runtimes"
        )

        expect(controller.unzoomSelectedTab(), "unzoom must clear the selected tab zoom state")
        expect(
            controller.displayPaneTree(for: controller.selectedTab)?.paneIDs == canonicalTree.paneIDs,
            "unzoom must restore the displayed split tree"
        )
    }

    private static func verifiesPortableZoomStateDoesNotChangePersistedSnapshotContract() {
        var state = ShellStateSnapshot.bootstrapDefault(
            windowID: "zoom_persistence_contract",
            workingDirectory: "/tmp"
        )
        state.zoomedPaneIDByTabID = ["tab_main": "pane_1"]

        guard let data = try? JSONEncoder().encode(state),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let decoded = try? JSONDecoder().decode(ShellStateSnapshot.self, from: data)
        else {
            fail("zoom persistence contract fixture must encode and decode")
        }

        expect(
            object["zoomed_pane_id_by_tab_id"] == nil
                && object["zoomedPaneIDByTabID"] == nil,
            "transient portable zoom state must not change persisted snapshot JSON"
        )
        expect(
            decoded.zoomedPaneIDByTabID.isEmpty,
            "persisted snapshots must restore transient zoom state from its durable owner"
        )
    }

    private static func verifiesTerminalRenderPrioritiesFollowSelectionAndZoom() {
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let controller = makeController(terminalRuntimeRegistry: registry)
        _ = controller.splitPane(paneID: "pane_1", placement: .right)

        let pane1 = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let pane2 = fakeSurfaceHandle(for: "pane_2", controller: controller)
        controller.focus(paneID: "pane_1")

        expect(
            pane1.renderPriority == .foregroundInteractive,
            "focused selected terminal must be foreground interactive"
        )
        expect(
            pane2.renderPriority == .visibleBackground,
            "visible split sibling must be visible background"
        )
        expect(
            pane1.renderCatchUpRequestCount == 1 && pane2.renderCatchUpRequestCount == 1,
            "initial promotion from hidden to visible priorities must request catch-up"
        )

        let firstTabID = controller.selectedTabID
        let secondTabID = controller.openTerminalTab(in: controller.selectedSpaceID)
        guard let hiddenPaneID = controller.selectedPane?.paneID else {
            fail("new terminal tab must select a pane")
        }
        let hiddenPane = fakeSurfaceHandle(for: hiddenPaneID, controller: controller)
        if let firstTabID {
            controller.select(tabID: firstTabID)
        }

        expect(secondTabID != nil, "test setup must open a second terminal tab")
        expect(
            hiddenPane.renderPriority == .hiddenBackground,
            "terminal in an unselected tab must be hidden background"
        )

        let pane2CatchUpsBeforeZoom = pane2.renderCatchUpRequestCount
        expect(controller.zoomPane(paneID: "pane_1"), "test setup must zoom focused split")
        expect(
            pane1.renderPriority == .foregroundInteractive,
            "zoomed focused terminal must remain foreground interactive"
        )
        expect(
            pane2.renderPriority == .hiddenBackground,
            "split sibling hidden by zoom must become hidden background"
        )

        expect(controller.unzoomSelectedTab(), "test setup must unzoom selected split")
        expect(
            pane2.renderPriority == .visibleBackground,
            "unzooming must promote visible split sibling back to visible background"
        )
        expect(
            pane2.renderCatchUpRequestCount == pane2CatchUpsBeforeZoom + 1,
            "hidden-to-visible unzoom transition must request catch-up for the sibling runtime"
        )
    }

    private static func verifiesTerminalRenderPrioritiesTrackWindowVisibility() {
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let controller = makeController(terminalRuntimeRegistry: registry)
        _ = controller.splitPane(paneID: "pane_1", placement: .right)

        let pane1 = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let pane2 = fakeSurfaceHandle(for: "pane_2", controller: controller)
        controller.focus(paneID: "pane_1")

        expect(
            pane1.renderPriority == .foregroundInteractive,
            "test setup must start with focused selected terminal foreground"
        )
        expect(
            pane2.renderPriority == .visibleBackground,
            "test setup must start with split sibling visible"
        )

        let pane1CatchUpsBeforeHide = pane1.renderCatchUpRequestCount
        let pane2CatchUpsBeforeHide = pane2.renderCatchUpRequestCount
        controller.updateShellWindowVisibilityForRendering(false)

        expect(
            pane1.renderPriority == .hiddenBackground,
            "hidden or occluded shell window must demote focused terminal rendering"
        )
        expect(
            pane2.renderPriority == .hiddenBackground,
            "hidden or occluded shell window must demote visible split rendering"
        )
        expect(
            pane1.renderCatchUpRequestCount == pane1CatchUpsBeforeHide
                && pane2.renderCatchUpRequestCount == pane2CatchUpsBeforeHide,
            "demoting window visibility must not request catch-up work"
        )

        controller.updateShellWindowVisibilityForRendering(true)
        expect(
            pane1.renderPriority == .foregroundInteractive,
            "restored visible shell window must promote focused terminal rendering"
        )
        expect(
            pane2.renderPriority == .visibleBackground,
            "restored visible shell window must promote split sibling rendering"
        )
        expect(
            pane1.renderCatchUpRequestCount == pane1CatchUpsBeforeHide + 1
                && pane2.renderCatchUpRequestCount == pane2CatchUpsBeforeHide + 1,
            "hidden-window to visible-window transition must request catch-up for visible terminals"
        )
    }

    private static func verifiesSplitZoomIsTabScopedAndPrunedWhenPaneDisappears() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let firstTabID = controller.selectedTabID
        expect(controller.zoomPane(paneID: "pane_1"), "test setup must zoom the first split tab")
        let secondTabID = controller.openTerminalTab(in: controller.selectedSpaceID)
        expect(secondTabID != nil, "test setup must open a second tab")

        expect(controller.selectedTabID == secondTabID, "opening a tab must select it")
        expect(controller.selectedTabZoomedPaneID == nil, "zoom state must not leak to another tab")
        if let firstTabID {
            controller.select(tabID: firstTabID)
        }
        expect(controller.selectedTabZoomedPaneID == "pane_1", "zoom state must remain attached to its tab")

        _ = controller.closePane(paneID: "pane_1")
        expect(controller.selectedTabZoomedPaneID == nil, "closing the zoomed pane must prune zoom state")
    }

    private static func verifiesInTabPaneMovementPreservesRuntimeContinuity() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let movedPaneBefore = controller.pane(paneID: "pane_2")
        _ = controller.terminalRuntimeRegistry.surfaceHandle(
            for: controller.pane(paneID: "pane_1"),
            bootProfile: controller.bootProfile(for: controller.pane(paneID: "pane_1"))
        )
        _ = controller.terminalRuntimeRegistry.surfaceHandle(
            for: controller.pane(paneID: "pane_2"),
            bootProfile: controller.bootProfile(for: controller.pane(paneID: "pane_2"))
        )
        let registeredBefore = controller.terminalRuntimeRegistry.registeredPaneIDs

        expect(
            controller.movePaneWithinTab(paneID: "pane_2", placement: .left),
            "in-tab movement must accept an adjacent destination"
        )
        expect(
            controller.selectedTab?.paneTree.paneIDs == ["pane_2", "pane_1"],
            "in-tab movement must update PaneSlot placement inside the selected tab"
        )
        expect(
            controller.pane(paneID: "pane_2") == movedPaneBefore,
            "in-tab movement must preserve mounted terminal content metadata"
        )
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs == registeredBefore,
            "in-tab movement must not release or recreate terminal runtimes"
        )
    }

    private static func verifiesPaneMovementDragPolicyProtectsTerminalSelection() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let originalTree = controller.selectedTab?.paneTree

        expect(
            !controller.movePaneWithinTab(
                paneID: "pane_2",
                placement: .left,
                source: .terminalContentDrag
            ),
            "terminal content drags must not start pane movement"
        )
        expect(
            controller.selectedTab?.paneTree == originalTree,
            "rejected terminal-content drag movement must leave layout unchanged"
        )
        expect(
            controller.movePaneWithinTab(
                paneID: "pane_2",
                placement: .left,
                source: .titleBarDragAffordance
            ),
            "drag-backed movement must route through the same controller mutation path"
        )
        expect(
            controller.selectedTab?.paneTree.paneIDs == ["pane_2", "pane_1"],
            "drag-backed movement must preserve the explicit movement result semantics"
        )
    }

    private static func verifiesTerminalCommandResolverRoutesFocusedTerminalCommands() {
        let controller = makeController()
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        handle.selectedText = "terminal selection"

        expect(
            controller.terminalCommandResolution(for: .copySelection).terminalTarget?.paneID == "pane_1",
            "keyboard copy must target the focused terminal when it owns a selection"
        )
        expect(
            controller.terminalCommandResolution(for: .copySelection, source: .commandUI)
                .terminalTarget?.paneID == "pane_1",
            "shared terminal resolver must keep source-specific callers on the focused terminal"
        )
    }

    private static func verifiesTerminalCommandResolverRejectsNonTerminalContent() {
        let controller = makeController()
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(
                fileURL: FileManager.default.temporaryDirectory
                    .appendingPathComponent("notes.md"),
                title: "Notes"
            )
        ) else {
            fail("test setup must create a markdown PaneSlot")
        }
        controller.focus(paneID: markdownPaneID)

        expect(
            controller.terminalCommandResolution(for: .paste, source: .commandUI)
                == .shell(reason: "terminal_content_unavailable"),
            "terminal commands must reject a focused non-terminal ContentInstance before runtime lookup"
        )
        expect(
            controller.shellActionAvailability(.findOpen)
                == .unavailable(reason: "Focused content is not a terminal"),
            "find action availability must be terminal-content scoped"
        )
    }

    private static func verifiesCopyPasteRouteToFocusedTerminalRuntime() {
        let systemPasteboard = FakeShellPasteboard(string: "system paste payload")
        let controller = makeController(pasteboard: systemPasteboard)
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        handle.selectedText = "focused terminal selection"
        let pasteboard = RecordingTerminalPasteboardWriter()

        expect(
            controller.copyTerminalSelection(source: .menuBar, writer: pasteboard),
            "menu copy must resolve to the focused terminal selection"
        )
        expect(
            pasteboard.string == "focused terminal selection",
            "menu copy must write the focused terminal selection"
        )

        expect(
            controller.pasteIntoTerminal("pasted payload", source: .menuBar),
            "menu paste must resolve to the focused terminal input path"
        )
        expect(
            handle.deliveredText.last == "pasted payload",
            "menu paste must deliver text through the terminal runtime owner"
        )

        expect(
            controller.pasteIntoTerminalFromPasteboard(source: .menuBar),
            "menu paste must read text through the injected system pasteboard boundary"
        )
        expect(
            handle.deliveredText.last == "system paste payload",
            "system pasteboard text must reach the focused terminal runtime"
        )
    }

    private static func verifiesContextMenuTerminalCommandsUseContextPane() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let focusedHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let contextHandle = fakeSurfaceHandle(for: "pane_2", controller: controller)
        focusedHandle.selectedText = "focused selection"
        contextHandle.selectedText = "context selection"
        controller.focus(paneID: "pane_1")

        let pasteboard = RecordingTerminalPasteboardWriter()
        expect(
            controller.copyTerminalSelection(
                source: .contextMenu,
                target: .contextPane("pane_2"),
                writer: pasteboard
            ),
            "context menu copy must use the context pane target"
        )
        expect(
            pasteboard.string == "context selection",
            "context menu copy must not fall back to the focused pane"
        )

        expect(
            controller.pasteIntoTerminal(
                "context paste",
                source: .contextMenu,
                target: .contextPane("pane_2")
            ),
            "context menu paste must use the context pane target"
        )
        expect(
            contextHandle.deliveredText.last == "context paste",
            "context menu paste must reach the context pane runtime"
        )
        expect(
            focusedHandle.deliveredText.isEmpty,
            "context menu paste must not deliver to the focused pane"
        )
    }

    private static func verifiesTerminalSearchRoutesThroughFocusedHostSurface() {
        let controller = makeController()
        guard let pane = controller.selectedPane else {
            fail("test setup must expose a selected pane")
        }
        let hostView = controller.terminalRuntimeRegistry.hostView(
            for: pane,
            bootProfile: controller.bootProfile(for: pane),
            isSelected: true,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { controller.updateTerminalRuntime($0) },
            onMetadataUpdate: { controller.updateTerminalMetadata($0, for: pane.paneID) }
        )
        let handle = fakeSurfaceHandle(for: pane.paneID, controller: controller)

        expect(
            controller.openTerminalSearch(source: .menuBar),
            "menu find must resolve to the focused terminal host surface"
        )
        expect(
            handle.searchActions == ["start_search"],
            "menu find must start search on the focused terminal ContentInstance"
        )
        expect(
            hostView.terminalCommandRuntimeState.searchAvailable,
            "terminal host path must publish search ownership for the resolver"
        )
    }

    private static func verifiesAdvancedControlPlaneResizeEqualizeAndEvents() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let splitNodeID = controller.selectedTab?.paneTree.splitNodes.first?.nodeID
        guard let splitNodeID else {
            fail("test setup must create a split node")
        }

        let resizeResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "resize-1",
                  "command": "pane.resize_split",
                  "split_node_id": "\(splitNodeID)",
                  "ratio": 0.99
                }
                """
            )
        )

        expect(resizeResponse.applied == true, "resize_split must report an applied result")
        expect(resizeResponse.splitNodeID == splitNodeID, "resize response must identify the split node")
        expect(
            abs((resizeResponse.ratio ?? 0) - 0.85) < 0.001,
            "resize response must include the effective clamped ratio"
        )
        expect(
            resizeResponse.affectedPaneIDs == ["pane_1", "pane_2"],
            "resize response must include affected pane ids"
        )
        expect(resizeResponse.latestEventID != nil, "resize response must include host event metadata")
        expect(
            controlEvents(controller).contains {
                $0.type == "split.ratio_changed"
                    && $0.payload["split_node_id"] == .string(splitNodeID)
            },
            "resize command must emit a split ratio event"
        )

        let equalizeResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "equalize-1",
                  "command": "pane.equalize_splits",
                  "tab_id": "\(controller.selectedTabID ?? "")"
                }
                """
            )
        )

        expect(equalizeResponse.applied == true, "equalize_splits must report an applied result")
        expect(equalizeResponse.ratio == 0.5, "equalize response must report the reset ratio")
        expect(
            equalizeResponse.changedSplitIDs == [splitNodeID],
            "equalize response must identify changed split ids"
        )
        expect(
            equalizeResponse.latestEventID != nil,
            "equalize response must include host event metadata"
        )
        expect(
            controlEvents(controller).contains {
                $0.type == "split.equalized"
                    && $0.payload["changed_split_ids"] == .array([.string(splitNodeID)])
            },
            "equalize command must emit an equalization event"
        )

        let unchangedResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "equalize-unchanged-1",
                  "command": "pane.equalize_splits",
                  "tab_id": "\(controller.selectedTabID ?? "")"
                }
                """
            )
        )
        expect(unchangedResponse.applied == false, "unchanged equalize must not claim mutation")
        expect(
            unchangedResponse.errorCode == "unchanged_state",
            "unchanged equalize must return a stable error code"
        )
    }

    private static func verifiesSplitRatioEventsUseAffectedPaneForBackgroundTabs() {
        let controller = makeController()
        guard let foregroundSpaceID = controller.selectedSpaceID,
              let foregroundTabID = controller.selectedTabID
        else {
            fail("bootstrap shell must expose a selected Space and tab")
        }
        guard let backgroundSpaceID = controller.createTerminalSpace(
            title: "Background",
            workingDirectory: "/background"
        ),
              let backgroundTabID = controller.selectedTabID,
              let backgroundPaneID = controller.shellState.panes(in: backgroundTabID).first?.paneID
        else {
            fail("test setup must create a background Space")
        }
        _ = controller.splitPane(paneID: backgroundPaneID, placement: .right)
        guard let splitNodeID = controller.shellState
            .tab(tabID: backgroundTabID)?
            .paneTree
            .splitNodes
            .first?
            .nodeID
        else {
            fail("test setup must create a split node in the background tab")
        }

        controller.select(spaceID: foregroundSpaceID)
        controller.select(tabID: foregroundTabID)
        let foregroundPaneID = controller.shellState.focusedPaneID
        expect(foregroundPaneID != nil, "foreground tab selection must focus a pane")

        let resizeResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "resize-background-1",
                  "command": "pane.resize_split",
                  "split_node_id": "\(splitNodeID)",
                  "ratio": 0.66
                }
                """
            )
        )

        expect(resizeResponse.applied == true, "background resize must apply")
        expect(
            resizeResponse.spaceID == backgroundSpaceID && resizeResponse.tabID == backgroundTabID,
            "background resize response must identify the target Space and tab"
        )
        expect(
            resizeResponse.affectedPaneIDs?.contains(resizeResponse.paneID ?? "") == true,
            "resize response pane_id must come from the affected split"
        )
        expect(
            resizeResponse.paneID != foregroundPaneID,
            "resize response pane_id must not use the unrelated focused pane"
        )
        guard let ratioEvent = controlEvents(controller).last(where: {
            $0.type == "split.ratio_changed"
                && $0.payload["split_node_id"] == .string(splitNodeID)
        }) else {
            fail("background resize must emit a split ratio event")
        }
        expect(ratioEvent.tabID == backgroundTabID, "split ratio event must stay tab-scoped")
        expect(
            resizeResponse.affectedPaneIDs?.contains(ratioEvent.paneID ?? "") == true,
            "split ratio event pane_id must come from the affected split"
        )
        expect(
            ratioEvent.paneID != foregroundPaneID,
            "split ratio event pane_id must not use the unrelated focused pane"
        )

        let equalizeResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "equalize-background-1",
                  "command": "pane.equalize_splits",
                  "tab_id": "\(backgroundTabID)"
                }
                """
            )
        )
        expect(equalizeResponse.applied == true, "background equalize must apply")
        expect(
            equalizeResponse.spaceID == backgroundSpaceID
                && equalizeResponse.tabID == backgroundTabID,
            "background equalize response must identify the target Space and tab"
        )
    }

    private static func verifiesAdvancedControlPlaneZoomFocusAndMovementResults() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        controller.focus(paneID: "pane_1")

        let zoomResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "zoom-1",
                  "command": "pane.zoom",
                  "pane_id": "pane_2"
                }
                """
            )
        )

        expect(zoomResponse.applied == true, "zoom command must apply to a valid split pane")
        expect(zoomResponse.zoomedPaneID == "pane_2", "zoom response must include tab zoom state")
        expect(
            zoomResponse.previousFocusedPaneID == "pane_1"
                && zoomResponse.currentFocusedPaneID == "pane_2",
            "zoom response must report previous/current focus"
        )
        expect(
            controlEvents(controller).contains {
                $0.type == "pane.zoom_changed"
                    && $0.payload["zoomed_pane_id"] == .string("pane_2")
            },
            "zoom command must emit a zoom state event"
        )

        let unzoomResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "unzoom-1",
                  "command": "pane.unzoom",
                  "tab_id": "\(controller.selectedTabID ?? "")"
                }
                """
            )
        )
        expect(unzoomResponse.applied == true, "unzoom command must clear zoom state")
        expect(unzoomResponse.zoomedPaneID == nil, "unzoom response must report cleared zoom state")
        expect(
            unzoomResponse.paneID == "pane_2"
                && unzoomResponse.mountedContentInstanceID
                    == ShellContentInstance.terminalContentID(forPaneID: "pane_2")
                && unzoomResponse.latestEventID != nil,
            "unzoom response must preserve the previous zoom target and event metadata"
        )

        let spatialResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "spatial-left-1",
                  "command": "pane.spatial_focus",
                  "spatial_direction": "left"
                }
                """
            )
        )
        expect(spatialResponse.applied == true, "spatial focus left must find the adjacent pane")
        expect(
            spatialResponse.previousFocusedPaneID == "pane_2"
                && spatialResponse.currentFocusedPaneID == "pane_1",
            "spatial focus response must report previous/current focus"
        )

        let noTargetResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "spatial-left-2",
                  "command": "pane.spatial_focus",
                  "spatial_direction": "left"
                }
                """
            )
        )
        expect(noTargetResponse.applied == false, "spatial focus must not apply without a target")
        expect(
            noTargetResponse.errorCode == "spatial_focus_target_not_found",
            "spatial focus must return a stable no-target error"
        )
        expect(
            noTargetResponse.previousFocusedPaneID == "pane_1"
                && noTargetResponse.currentFocusedPaneID == "pane_1",
            "no-target spatial focus must preserve focus"
        )
        expect(
            noTargetResponse.latestEventID != nil,
            "no-target spatial focus must include its recorded host event metadata"
        )

        let moveResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "move-within-1",
                  "command": "pane.move_within_tab",
                  "pane_id": "pane_2",
                  "placement": "left"
                }
                """
            )
        )
        expect(moveResponse.applied == true, "move_within_tab must apply to an adjacent target")
        expect(
            moveResponse.latestEventID != nil,
            "move_within_tab response must include host event metadata"
        )
        expect(moveResponse.sourceTabID == moveResponse.targetTabID, "in-tab move must stay in one tab")
        expect(
            moveResponse.mountedContentInstanceID
                == ShellContentInstance.terminalContentID(forPaneID: "pane_2"),
            "movement response must report preserved mounted content identity"
        )
        expect(
            controlEvents(controller).contains {
                $0.type == "pane.moved_in_tab"
                    && $0.payload["mounted_content_instance_id"] == .string("pane_2")
            },
            "in-tab movement must emit an advanced movement event"
        )
    }

    private static func verifiesAdvancedControlPlaneRejectsUnknownUnzoomPane() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        _ = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "zoom-before-invalid-unzoom-1",
                  "command": "pane.zoom",
                  "pane_id": "pane_2"
                }
                """
            )
        )
        expect(controller.selectedTabZoomedPaneID == "pane_2", "test setup must zoom a pane")

        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "invalid-unzoom-pane-1",
                  "command": "pane.unzoom",
                  "pane_id": "pane_missing"
                }
                """
            )
        )

        expect(response.applied == false, "unzoom with an unknown explicit pane must not apply")
        expect(response.errorCode == "pane_not_found", "unknown unzoom pane must return pane_not_found")
        expect(response.paneID == "pane_missing", "unknown unzoom pane response must echo pane_id")
        expect(
            controller.selectedTabZoomedPaneID == "pane_2",
            "unknown unzoom pane must not fall back to and mutate the selected tab"
        )
    }

    private static func verifiesShellCoreUnavailableFallbackIsReadOnly() {
        let readOnlyFallbackCommands: Set<AlanShellControlCommandKind> = [
            .state,
            .spaceList,
            .tabList,
            .paneList,
        ]
        let mutationCommands: [AlanShellControlCommandKind] = [
            .spaceCreate,
            .tabOpen,
            .tabClose,
            .tabReorder,
            .tabPin,
            .tabUnpin,
            .tabMoveToSpace,
            .paneSplit,
            .paneClose,
            .paneLift,
            .paneMove,
            .paneMoveWithinTab,
            .paneFocus,
            .paneSpatialFocus,
            .paneResizeSplit,
            .paneEqualizeSplits,
            .paneZoom,
            .paneUnzoom,
            .attentionSet,
        ]

        for command in readOnlyFallbackCommands {
            expect(
                command.canFallThroughToHostWhenShellCoreUnavailable,
                "\(command.rawValue) must fall through only for host-readable snapshots"
            )
        }
        for command in mutationCommands {
            expect(
                !command.canFallThroughToHostWhenShellCoreUnavailable,
                "\(command.rawValue) must fail closed instead of falling through to host mutation"
            )
        }
    }

    private static func verifiesPaneMoveSocketRequestsUseSharedExecutor() {
        let controller = makeController()
        guard let targetTabID = controller.openTerminalTab() else {
            fail("pane.move socket test must create a target tab")
        }
        let initialState = controller.shellState
        var adoptedState: ShellStateSnapshot?
        let socketServer = AlanShellSocketServer(
            socketURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("pane-move-shared-\(UUID().uuidString).sock"),
            commandHandler: { _ in fail("portable pane.move must not require the host handler") },
            stateAdoptionHandler: { adoptedState = $0 },
            sideEffectHandler: { _ in fail("pane.move must not use local side effects") }
        )
        _ = socketServer.mergePublishedState(initialState)

        let localResponse = socketServer.handleLocally(
            decodeControlCommand(
                """
                {
                  "request_id": "pane-move-shared-routing-1",
                  "command": "pane.move",
                  "pane_id": "pane_1",
                  "tab_id": "\(targetTabID)"
                }
                """
            )
        )

        expect(
            localResponse?.applied == true
                && localResponse?.sourceTabID == "tab_main"
                && localResponse?.targetTabID == targetTabID,
            "pane.move socket requests must use shared executor response semantics"
        )
        expect(
            adoptedState?.pane(paneID: "pane_1")?.tabID == targetTabID,
            "pane.move socket requests must adopt the shared executor state"
        )
    }

    private static func verifiesPaneResizeSocketRequestsRequireHostHandler() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        guard let splitNodeID = controller.selectedTab?.paneTree.splitNodes.first?.nodeID else {
            fail("resize socket routing test must create a split node")
        }
        let socketServer = AlanShellSocketServer(
            socketURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("pane-resize-host-\(UUID().uuidString).sock"),
            commandHandler: { controller.handleControlPlaneCommand($0) },
            stateAdoptionHandler: { _ in
                fail("pane.resize_split must not bypass host response enrichment")
            },
            sideEffectHandler: { _ in
                fail("pane.resize_split must not emit a socket-local side effect")
            }
        )
        _ = socketServer.mergePublishedState(controller.shellState)

        let localResponse = socketServer.handleLocally(
            decodeControlCommand(
                """
                {
                  "request_id": "pane-resize-host-routing-1",
                  "command": "pane.resize_split",
                  "split_node_id": "\(splitNodeID)",
                  "ratio": 0.63
                }
                """
            )
        )

        expect(
            localResponse == nil,
            "pane.resize_split socket requests must route through host event enrichment"
        )
    }

    private static func verifiesTerminalSendTextSocketRequestsRequireHostHandler() {
        let controller = makeController()
        let socketServer = AlanShellSocketServer(
            socketURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("terminal-send-text-host-\(UUID().uuidString).sock"),
            commandHandler: { controller.handleControlPlaneCommand($0) },
            stateAdoptionHandler: { _ in
                fail("terminal.send_text must not mutate through the local executor")
            },
            sideEffectHandler: { _ in
                fail("terminal.send_text must not use socket-local side effects")
            }
        )
        _ = socketServer.mergePublishedState(controller.shellState)

        let localResponse = socketServer.handleLocally(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-send-text-host-routing-1",
                  "command": "terminal.send_text",
                  "pane_slot_id": "pane_1",
                  "text": "echo host-routing"
                }
                """
            )
        )

        expect(
            localResponse == nil,
            "terminal.send_text socket requests must be routed to the host handler"
        )
    }

    private static func verifiesTerminalSendKeySocketRequestsRequireHostHandler() {
        let controller = makeController()
        let socketServer = AlanShellSocketServer(
            socketURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("terminal-send-key-host-\(UUID().uuidString).sock"),
            commandHandler: { controller.handleControlPlaneCommand($0) },
            stateAdoptionHandler: { _ in
                fail("terminal.send_key must not mutate through the local executor")
            },
            sideEffectHandler: { _ in
                fail("terminal.send_key must not use socket-local side effects")
            }
        )
        _ = socketServer.mergePublishedState(controller.shellState)

        let localResponse = socketServer.handleLocally(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-send-key-host-routing-1",
                  "command": "terminal.send_key",
                  "pane_slot_id": "pane_1",
                  "key": "return"
                }
                """
            )
        )

        expect(
            localResponse == nil,
            "terminal.send_key socket requests must be routed to the host handler"
        )
    }

    private static func verifiesTerminalActivityProjectsByPaneID() {
        let controller = makeController()
        _ = controller.openTerminalTab(workingDirectory: "/background")
        let activity = progressActivity(
            percent: 42,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )

        controller.updateTerminalMetadata(
            metadata(title: "build", cwd: "/repo/app", activity: activity),
            for: "pane_1"
        )

        let activePane = controller.pane(paneID: "pane_1")
        let backgroundPane = controller.pane(paneID: "pane_2")
        expect(
            activePane?.activity == activity,
            "terminal activity metadata must project onto the owning pane"
        )
        expect(
            backgroundPane?.activity == nil,
            "terminal activity metadata must not leak to other panes"
        )
    }

    private static func verifiesTerminalActivitySidebarPriority() {
        let running = activity(status: .running, source: .shell, sourceLabel: "Shell", stateLabel: "Running")
        let progress = activity(
            status: .progress,
            source: .progress,
            sourceLabel: "Progress",
            stateLabel: "42%",
            progress: .percent(42)
        )
        let needsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed"
        )

        expect(
            TerminalActivitySnapshot.primarySidebarActivity([running, progress]) == progress,
            "progress must outrank generic running activity"
        )
        expect(
            TerminalActivitySnapshot.primarySidebarActivity([progress, needsInput]) == needsInput,
            "user-input-required activity must outrank progress"
        )
    }

    private static func verifiesProgressActivityFactoryUsesSourceFirstDisplay() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let activity = TerminalActivitySnapshot.progressActivity(percent: 42, now: now)

        expect(activity.source.kind == .progress, "progress factory must label source as progress")
        expect(activity.status == .progress, "determinate progress must use progress status")
        expect(activity.progress == .percent(42), "determinate progress must carry bounded percent")
        expect(
            activity.display.sourceFirstLabel == "Progress · 42%",
            "progress display copy must be source-first"
        )
        expect(
            activity.freshness.staleAt == "2026-05-17T09:00:15Z",
            "progress activity must get the default 15 second stale deadline"
        )
    }

    private static func verifiesCodexAgentActivityAdapterMapsSupportedStates() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let updatedAt = "2026-05-17T09:00:00Z"
        let running = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "codex",
                status: "running",
                sessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan",
                detail: nil,
                updatedAt: updatedAt
            ),
            now: now
        )
        let needsInput = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "codex",
                status: "approval_required",
                sessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan",
                detail: nil,
                updatedAt: updatedAt
            ),
            now: now
        )
        let complete = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "codex",
                status: "completed",
                sessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan",
                detail: nil,
                updatedAt: updatedAt
            ),
            now: now
        )
        let failed = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "codex",
                status: "error",
                sessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan",
                detail: nil,
                updatedAt: updatedAt
            ),
            now: now
        )

        expect(running?.source.kind == .codex, "Codex running activity must keep codex as source")
        expect(running?.status == .running, "Codex running event must map to running")
        expect(running?.display.sourceFirstLabel == "Codex · Running", "Codex running copy must be source-first")
        expect(running?.freshness.staleAt == "2026-05-17T09:01:30Z", "running agent activity must have a bounded stale window")
        expect(needsInput?.status == .needsInput, "Codex approval-required event must map to needs-input")
        expect(needsInput?.priority == .awaitingUser, "Codex needs-input event must be awaiting-user priority")
        expect(needsInput?.freshness.staleAt == nil, "Codex needs-input must persist until replaced")
        expect(complete?.status == .done, "Codex completed event must map to done")
        expect(complete?.freshness.expiresAt == "2026-05-17T09:00:08Z", "Codex done activity must be brief")
        expect(failed?.status == .failed, "Codex error event must map to failed")
        expect(failed?.display.sourceFirstLabel == "Codex · Error", "Codex error copy must hide implementation names")
        expect(failed?.freshness.staleAt == nil, "Codex error must persist until replaced")
    }

    private static func verifiesAgentActivityAdapterSanitizesDefaultUIPayload() {
        let activity = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "codex",
                status: "needs_input",
                sessionLabel: "session-1234567890abcdef1234567890",
                projectLabel: "alan\nworkspace",
                workingDirectory: "/Users/morris/Developer/alan",
                detail: #"{"event":"codex.status","session_id":"session-1234567890abcdef1234567890"}"#,
                updatedAt: "2026-05-17T09:00:00Z"
            ),
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )

        expect(activity?.agent?.safeSessionLabel == nil, "agent adapter must not expose raw session ids")
        expect(activity?.agent?.projectLabel == "alan workspace", "agent adapter must collapse control characters in labels")
        expect(activity?.display.detailLabel == nil, "agent adapter must not expose raw hook payloads in default UI detail")
        if let activity {
            do {
                let data = try JSONEncoder().encode(activity)
                let json = String(data: data, encoding: .utf8) ?? ""
                expect(!json.contains("codex.status"), "serialized activity must not retain raw hook event names")
                expect(!json.contains("1234567890abcdef"), "serialized activity must not retain raw session ids")
            } catch {
                fail("agent activity JSON encode failed: \(error)")
            }
        } else {
            fail("valid Codex needs-input activity should adapt")
        }
    }

    private static func verifiesAgentActivityAdapterRejectsMalformedPayloadAndFallsBackForUnsupportedAgent() {
        let malformed = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "codex",
                status: "tool_call_delta",
                sessionLabel: nil,
                projectLabel: nil,
                workingDirectory: nil,
                detail: nil,
                updatedAt: "2026-05-17T09:00:00Z"
            ),
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )
        let unsupported = TerminalAgentActivityAdapter.activity(
            from: TerminalAgentActivityEvent(
                agentKind: "future-agent",
                status: "running",
                sessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan",
                detail: nil,
                updatedAt: "2026-05-17T09:00:00Z"
            ),
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )

        expect(malformed == nil, "unknown implementation event names must not create precise activity")
        expect(unsupported?.source.kind == .unknown, "unsupported agents must fall back to unknown source")
        expect(unsupported?.display.sourceFirstLabel == "Agent · Running", "unsupported agents must use generic UI copy")
    }

    private static func verifiesAgentActivityControlCommandProjectsOntoPane() {
        let controller = makeController(appIsActive: false)
        let json = """
        {
          "request_id": "agent-activity-1",
          "command": "agent.activity",
          "pane_id": "pane_1",
          "agent_kind": "codex",
          "agent_status": "needs_input",
          "session_label": "session-1234567890abcdef1234567890",
          "project_label": "alan",
          "working_directory": "/Users/morris/Developer/alan",
          "detail": "{\\"event\\":\\"codex.status\\"}",
          "updated_at": "2026-05-17T09:00:00Z"
        }
        """
        let command = decodeControlCommand(json)
        let response = controller.handleControlPlaneCommand(command)
        let paneActivity = controller.pane(paneID: "pane_1")?.activity

        expect(response.applied == true, "agent activity command must be applied")
        expect(response.paneID == "pane_1", "agent activity command must identify the updated pane")
        expect(paneActivity?.source.kind == .codex, "agent activity command must project Codex source onto pane")
        expect(paneActivity?.status == .needsInput, "agent activity command must project needs-input status")
        expect(paneActivity?.agent?.safeSessionLabel == nil, "control command projection must not expose raw session ids")
        expect(controller.activityNotifications.first?.kind == .needsInput, "agent command must reuse low-noise notification routing")
    }

    private static func verifiesCommandCompletionActivityFactory() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let success = TerminalActivitySnapshot.commandCompletion(exitCode: 0, now: now)
        let failure = TerminalActivitySnapshot.commandCompletion(exitCode: 2, now: now)
        let longSuccess = TerminalActivitySnapshot.commandCompletion(
            exitCode: 0,
            now: now,
            durationMilliseconds: 120_000
        )

        expect(success.status == .done, "zero exit code must produce done status")
        expect(!success.isSidebarWorthy, "successful commands must not be sidebar-worthy")
        expect(
            success.freshness.staleAt == "2026-05-17T09:00:08Z",
            "successful command completion must get a short stale deadline"
        )
        expect(
            !success.isFresh(at: now.addingTimeInterval(9)),
            "successful command completion must become stale after its freshness window"
        )
        expect(
            longSuccess.command?.durationMilliseconds == 120_000,
            "command completion must preserve measured duration"
        )
        expect(failure.status == .failed, "non-zero exit code must produce failed status")
        expect(failure.command?.exitCode == 2, "command completion must preserve exit code")
        expect(
            failure.display.sourceFirstLabel == "Shell · Command failed 2",
            "failed command copy must be source-first"
        )
        expect(
            TerminalActivitySnapshot.primarySidebarActivity([success, failure], now: now) == failure,
            "failed command completion must outrank successful completion"
        )
    }

    private static func verifiesTerminalActivityCodableUsesSnakeCase() {
        let activity = TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: .codex, label: "Codex"),
            status: .failed,
            priority: .notable,
            progress: nil,
            command: TerminalActivityCommandOutcome(
                exitCode: 2,
                durationMilliseconds: 120_000,
                commandText: "just check"
            ),
            agent: TerminalActivityAgentMetadata(
                kind: .codex,
                safeSessionLabel: "Codex",
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            ),
            display: TerminalActivityDisplay(
                sourceLabel: "Codex",
                stateLabel: "Failed",
                detailLabel: "just check",
                paneHint: "1"
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: "2026-05-17T09:00:00Z",
                staleAt: "2026-05-17T09:00:30Z",
                expiresAt: "2026-05-17T09:05:00Z"
            )
        )

        do {
            let data = try JSONEncoder().encode(activity)
            guard
                let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                let command = root["command"] as? [String: Any],
                let agent = root["agent"] as? [String: Any],
                let display = root["display"] as? [String: Any],
                let freshness = root["freshness"] as? [String: Any]
            else {
                fail("activity JSON must encode nested objects")
            }

            expect(command["exit_code"] as? Int == 2, "command JSON must use exit_code")
            expect(
                command["duration_milliseconds"] as? Int == 120_000,
                "command JSON must use duration_milliseconds"
            )
            expect(command["command_text"] as? String == "just check", "command JSON must use command_text")
            expect(!command.keys.contains("exitCode"), "command JSON must not use camelCase exitCode")
            expect(
                agent["safe_session_label"] as? String == "Codex",
                "agent JSON must use safe_session_label"
            )
            expect(agent["project_label"] as? String == "alan", "agent JSON must use project_label")
            expect(
                agent["working_directory"] as? String == "/Users/morris/Developer/alan",
                "agent JSON must use working_directory"
            )
            expect(display["source_label"] as? String == "Codex", "display JSON must use source_label")
            expect(display["state_label"] as? String == "Failed", "display JSON must use state_label")
            expect(display["detail_label"] as? String == "just check", "display JSON must use detail_label")
            expect(display["pane_hint"] as? String == "1", "display JSON must use pane_hint")
            expect(
                freshness["updated_at"] as? String == "2026-05-17T09:00:00Z",
                "freshness JSON must use updated_at"
            )
            expect(
                freshness["stale_at"] as? String == "2026-05-17T09:00:30Z",
                "freshness JSON must use stale_at"
            )
            expect(
                freshness["expires_at"] as? String == "2026-05-17T09:05:00Z",
                "freshness JSON must use expires_at"
            )

            let decoded = try JSONDecoder().decode(TerminalActivitySnapshot.self, from: data)
            expect(decoded == activity, "activity JSON must round-trip through snake_case keys")
        } catch {
            fail("activity JSON contract failed: \(error)")
        }
    }

    private static func verifiesSuccessfulCommandIsNotSidebarWorthy() {
        let success = activity(
            status: .done,
            source: .command,
            sourceLabel: "Shell",
            stateLabel: "Command succeeded"
        )

        expect(
            TerminalActivitySnapshot.primarySidebarActivity([success]) == nil,
            "successful command completion must not become sidebar-worthy activity"
        )
    }

    private static func verifiesStaleProgressIsNotSidebarWorthy() {
        let progress = progressActivity(
            percent: 42,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )
        let now = Date(timeIntervalSince1970: 1_779_008_416)

        expect(
            !progress.isFresh(at: now),
            "progress must become stale after its freshness deadline"
        )
        expect(
            TerminalActivitySnapshot.primarySidebarActivity([progress], now: now) == nil,
            "stale progress must not remain sidebar-worthy"
        )
    }

    private static func verifiesDefaultSidebarActivitySelectionHonorsFreshness() {
        let staleProgress = progressActivity(
            percent: 42,
            updatedAt: "2000-01-01T00:00:00Z",
            staleAt: "2000-01-01T00:00:15Z"
        )

        expect(
            TerminalActivitySnapshot.primarySidebarActivity([staleProgress]) == nil,
            "default sidebar activity selection must reject stale activity"
        )
    }

    private static func verifiesClearingActivityRemovesPaneActivity() {
        let controller = makeController()
        let progress = progressActivity(
            percent: 64,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )

        controller.updateTerminalMetadata(
            metadata(title: "build", cwd: "/repo/app", activity: progress),
            for: "pane_1"
        )
        expect(
            controller.shellState.pane(paneID: "pane_1")?.activity == progress,
            "test setup must project progress activity"
        )

        controller.updateTerminalMetadata(
            metadata(title: "build", cwd: "/repo/app", clearsActivity: true),
            for: "pane_1"
        )
        expect(
            controller.shellState.pane(paneID: "pane_1")?.activity == nil,
            "clear metadata must remove stale pane activity"
        )
    }

    private static func verifiesPublishedStateMergeClearsActivity() {
        let progress = progressActivity(
            percent: 42,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )
        let authoritative = stateWithAlanBinding(
            windowID: "window_activity_merge",
            pendingRequest: false,
            activity: progress
        )
        let incoming = stateWithAlanBinding(
            windowID: "window_activity_merge",
            pendingRequest: false,
            activity: nil
        )

        let merged = AlanShellPublishedStateMerger.merge(
            authoritative: authoritative,
            incoming: incoming
        )

        expect(
            merged.pane(paneID: "pane_1")?.activity == nil,
            "published state merge must allow incoming nil activity to clear stale activity"
        )
    }

    private static func verifiesPublishedStateMergeClearsTerminalProfileMetadata() {
        let staleContext = context(
            processState: "running",
            rendererHealth: "healthy",
            surfaceReadiness: "ready",
            terminalProfileState: "resolved",
            terminalProfileRequestedID: "alan",
            terminalProfileID: "alan",
            terminalProfileKind: "sudo_user",
            terminalProfileTitle: "Alan",
            lastCommandExitCode: nil
        )
        let incomingContext = context(
            processState: "running",
            rendererHealth: "healthy",
            surfaceReadiness: "ready",
            terminalProfileState: "missing",
            terminalProfileRequestedID: "lab",
            terminalProfileID: nil,
            terminalProfileKind: nil,
            terminalProfileTitle: nil,
            lastCommandExitCode: nil
        )
        let authoritative = stateWithContext(
            windowID: "window_terminal_profile_merge",
            context: staleContext
        )
        let incoming = stateWithContext(
            windowID: "window_terminal_profile_merge",
            context: incomingContext
        )

        let merged = AlanShellPublishedStateMerger.merge(
            authoritative: authoritative,
            incoming: incoming
        )
        let mergedContext = merged.pane(paneID: "pane_1")?.context

        expect(
            mergedContext?.terminalProfileState == "missing",
            "published state merge must accept incoming terminal profile resolution state"
        )
        expect(
            mergedContext?.terminalProfileRequestedID == "lab",
            "published state merge must accept incoming terminal profile requested id"
        )
        expect(
            mergedContext?.terminalProfileID == nil,
            "published state merge must clear stale resolved terminal profile id"
        )
        expect(
            mergedContext?.terminalProfileKind == nil,
            "published state merge must clear stale resolved terminal profile kind"
        )
        expect(
            mergedContext?.terminalProfileTitle == nil,
            "published state merge must clear stale resolved terminal profile title"
        )

        let authoritativeProfileState = stateWithContext(
            windowID: "window_terminal_profile_refs",
            context: staleContext,
            spaceTerminalProfileID: "alan",
            paneTerminalProfileID: "alan"
        )
        let incomingProfileState = stateWithContext(
            windowID: "window_terminal_profile_refs",
            context: incomingContext
        )
        let mergedProfileRefs = AlanShellPublishedStateMerger.merge(
            authoritative: authoritativeProfileState,
            incoming: incomingProfileState
        )
        expect(
            mergedProfileRefs.space(spaceID: "space_1")?.terminalProfileID == nil,
            "published state merge must let incoming nil clear a Space terminal profile id"
        )
        expect(
            mergedProfileRefs.pane(paneID: "pane_1")?.terminalProfileID == "alan",
            "published state merge must preserve captured pane terminal profile id"
        )
    }

    private static func verifiesPublishedStateMergePreservesSpaceSelection() {
        let controller = makeController()
        _ = controller.openTerminalTab(title: "Main Second")
        _ = controller.createTerminalSpace(title: "Other", workingDirectory: "/tmp")
        let authoritative = controller.shellState

        let incomingSpaces = authoritative.spaces.map { space in
            ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs,
                terminalProfileID: space.terminalProfileID
            )
        }
        let incoming = ShellStateSnapshot(
            contractVersion: authoritative.contractVersion,
            windowID: authoritative.windowID,
            focusedSpaceID: authoritative.focusedSpaceID,
            focusedTabID: authoritative.focusedTabID,
            focusedPaneID: authoritative.focusedPaneID,
            spaces: incomingSpaces,
            panes: authoritative.panes,
            paneSlots: authoritative.paneSlots,
            contents: authoritative.contents
        )

        let merged = AlanShellPublishedStateMerger.merge(
            authoritative: authoritative,
            incoming: incoming
        )

        expect(
            merged.space(spaceID: "space_main")?.selectedTabID == "tab_2",
            "published state merge must preserve inactive Space tab selection from authoritative state"
        )
        expect(
            merged.space(spaceID: "space_2")?.selectedTabID == "tab_3",
            "published state merge must keep the focused Space selection aligned with incoming focus"
        )
    }

    private static func verifiesPublishedStateMergePreservesContentContainers() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("PublishedStateMerge-\(UUID().uuidString).md")
        do {
            try "## Merge notes\n".write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("published state merge setup must create markdown file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let baseline = makeController().shellState
        let splitResult: ShellStateMutationResult
        do {
            splitResult = try baseline.splittingPane(
                "pane_1",
                placement: .right,
                contentIntent: .markdown(fileURL: fileURL, title: "Merge Notes")
            )
        } catch {
            fail("published state merge setup must create markdown split: \(error)")
        }
        guard let paneSlotID = splitResult.paneID else {
            fail("published state merge setup must return new PaneSlot id")
        }

        let merged = AlanShellPublishedStateMerger.merge(
            authoritative: baseline,
            incoming: splitResult.state
        )
        let mergedContentState = merged.contentStateProjection()

        expect(
            merged.paneSlots != nil && merged.contents != nil,
            "published state merge must retain explicit content container records"
        )
        expect(
            mergedContentState.contentMounted(in: paneSlotID)?.kind == .markdown,
            "published state merge must not project markdown content back into terminal content"
        )
    }

    private static func verifiesPaneRebuildMutationsPreserveActivity() {
        let controller = makeController()
        let progress = progressActivity(
            percent: 42,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )

        controller.updateTerminalMetadata(
            metadata(title: "build", cwd: "/repo/app", activity: progress),
            for: "pane_1"
        )
        expect(
            controller.pane(paneID: "pane_1")?.activity == progress,
            "test setup must project progress activity before pane rebuilds"
        )

        _ = controller.setAttention(.notable, for: "pane_1")
        expect(
            controller.pane(paneID: "pane_1")?.activity == progress,
            "attention-only pane rebuild must preserve terminal activity"
        )

        guard let targetTabID = controller.openTerminalTab(workingDirectory: "/target") else {
            fail("test setup must create a target tab")
        }
        expect(
            controller.movePane(paneID: "pane_1", toTab: targetTabID, direction: .vertical),
            "test setup must move pane into target tab"
        )
        expect(
            controller.pane(paneID: "pane_1")?.activity == progress,
            "pane move rebuild must preserve terminal activity"
        )

        switch controller.liftPaneToTab(paneID: "pane_1") {
        case .lifted:
            break
        case .paneNotFound, .lastPane:
            fail("test setup must lift moved pane into a new tab")
        }
        expect(
            controller.pane(paneID: "pane_1")?.activity == progress,
            "pane lift rebuild must preserve terminal activity"
        )
    }

    private static func verifiesTabSidebarActivityProjectionUsesHighestPriorityPane() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let progress = progressActivity(
            percent: 42,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )
        let needsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed",
            updatedAt: "2026-05-17T09:00:01Z"
        )

        controller.updateTerminalMetadata(
            metadata(title: "build", cwd: "/repo/app", activity: progress),
            for: "pane_1"
        )
        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: needsInput),
            for: "pane_2"
        )

        guard let tab = controller.shellState.tab(tabID: "tab_main") else {
            fail("test setup must keep the split tab")
        }

        let projection = shellSidebarTabProjection(
            for: tab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_1",
            focusedTabID: "tab_other",
            now: now
        )

        expect(projection.activity?.status == .needsInput, "tab activity must pick the most actionable pane")
        expect(
            projection.secondaryLine?.hasPrefix("Input needed") == true
                && projection.secondaryLine?.contains("Pane 2") == true
                && projection.secondaryLine?.contains("Codex") == true,
            "background pane activity must put actionable state first and retain a short pane hint"
        )
        expect(projection.progress == nil, "non-progress displayed activity must not inherit another pane progress")
    }

    private static func verifiesTabSidebarProjectionFallsBackToRepositoryBranch() {
        let tab = ShellTab(
            tabID: "tab_1",
            kind: .terminal,
            title: nil,
            paneTree: ShellPaneTreeNode(
                nodeID: "node_pane_1",
                kind: .pane,
                direction: nil,
                paneID: "pane_1",
                children: nil
            )
        )
        let testPane = pane(
            context: context(
                workingDirectoryName: "src",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "main",
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            cwd: "/Users/morris/Developer/alan/crates/agent-engine",
            attention: .idle
        )

        let projection = shellSidebarTabProjection(
            for: tab,
            panes: [testPane],
            focusedPaneID: "pane_1",
            focusedTabID: "tab_1",
            now: nil
        )

        expect(projection.activity == nil, "idle panes must not produce tab activity")
        expect(
            projection.secondaryLine == "alan · main",
            "sidebar context fallback must prefer repository/worktree leaf plus branch"
        )
    }

    private static func verifiesTabSidebarProjectionPreservesTerminalStatusBeforeContext() {
        let tab = ShellTab(
            tabID: "tab_1",
            kind: .terminal,
            title: nil,
            paneTree: ShellPaneTreeNode(
                nodeID: "node_pane_1",
                kind: .pane,
                direction: nil,
                paneID: "pane_1",
                children: nil
            )
        )
        let failedRenderer = pane(
            context: context(
                workingDirectoryName: "src",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "main",
                processState: "running",
                rendererHealth: "failed",
                surfaceReadiness: "renderer_failed",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            cwd: "/Users/morris/Developer/alan/crates/agent-engine",
            attention: .notable
        )
        let startingPane = pane(
            context: context(
                workingDirectoryName: "src",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "main",
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "input_not_ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            cwd: "/Users/morris/Developer/alan/crates/agent-engine",
            attention: .idle
        )

        let failedProjection = shellSidebarTabProjection(
            for: tab,
            panes: [failedRenderer],
            focusedPaneID: "pane_1",
            focusedTabID: "tab_1",
            now: nil
        )
        let startingProjection = shellSidebarTabProjection(
            for: tab,
            panes: [startingPane],
            focusedPaneID: "pane_1",
            focusedTabID: "tab_1",
            now: nil
        )

        expect(
            failedProjection.secondaryLine == "Renderer failed",
            "sidebar fallback must preserve renderer failures before repository context"
        )
        expect(
            startingProjection.secondaryLine == "Starting",
            "sidebar fallback must preserve startup/input readiness before repository context"
        )
    }

    private static func verifiesTabSidebarProjectionDoesNotResurrectStaleCommandFailure() {
        let tab = ShellTab(
            tabID: "tab_1",
            kind: .terminal,
            title: nil,
            paneTree: ShellPaneTreeNode(
                nodeID: "node_pane_1",
                kind: .pane,
                direction: nil,
                paneID: "pane_1",
                children: nil
            )
        )
        let staleFailure = activity(
            status: .failed,
            source: .command,
            sourceLabel: "Shell",
            stateLabel: "Command failed 2",
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:30Z"
        )
        let testPane = pane(
            context: context(
                workingDirectoryName: "src",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "main",
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: 2
            ),
            viewport: ShellViewportSnapshot(
                title: "fish",
                summary: "command failed (2)",
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            cwd: "/Users/morris/Developer/alan/crates/agent-engine",
            attention: .notable,
            activity: staleFailure
        )

        let projection = shellSidebarTabProjection(
            for: tab,
            panes: [testPane],
            focusedPaneID: "pane_1",
            focusedTabID: "tab_1",
            now: Date(timeIntervalSince1970: 1_779_008_431)
        )

        expect(projection.activity == nil, "stale command failure must not remain sidebar activity")
        expect(
            projection.secondaryLine == "alan · main",
            "stale command failure summary must not hide repository context"
        )
    }

    private static func verifiesSidebarProgressRailBelongsToDisplayedActivity() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let progress = progressActivity(
            percent: 64,
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:15Z"
        )
        let failed = activity(
            status: .failed,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Error",
            updatedAt: "2026-05-17T09:00:01Z"
        )

        controller.updateTerminalMetadata(
            metadata(title: "build", cwd: "/repo/app", activity: progress),
            for: "pane_1"
        )
        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: failed),
            for: "pane_2"
        )

        guard let tab = controller.shellState.tab(tabID: "tab_main") else {
            fail("test setup must keep the split tab")
        }

        let failedProjection = shellSidebarTabProjection(
            for: tab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_1",
            focusedTabID: "tab_other",
            now: now
        )
        expect(
            failedProjection.secondaryLine?.hasPrefix("Error") == true
                && failedProjection.secondaryLine?.contains("Pane 2") == true
                && failedProjection.secondaryLine?.contains("Codex") == true,
            "failed activity must outrank progress and keep the background pane hint"
        )
        expect(failedProjection.progress == nil, "progress rail must not be shown for a different pane's progress")

        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", clearsActivity: true),
            for: "pane_2"
        )
        let progressProjection = shellSidebarTabProjection(
            for: tab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_1",
            focusedTabID: "tab_other",
            now: now
        )
        expect(progressProjection.secondaryLine == "Progress · 64%", "progress activity must become visible after failure clears")
        expect(progressProjection.progress == .percent(64), "progress rail must use the displayed activity progress")
    }

    private static func verifiesFocusedCommandFailureDemotesFromSidebarProjection() {
        let controller = makeController()
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let failure = TerminalActivitySnapshot.commandCompletion(exitCode: 2, now: now)
        controller.updateTerminalMetadata(
            metadata(title: "fish", cwd: "/Users/morris/Developer/alan", activity: failure),
            for: "pane_1"
        )

        guard let tab = controller.shellState.tab(tabID: "tab_main") else {
            fail("test setup must keep the tab")
        }

        let focusedProjection = shellSidebarTabProjection(
            for: tab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_1",
            focusedTabID: "tab_main",
            now: now
        )
        expect(focusedProjection.activity == nil, "focused command failure may demote from sidebar activity")

        let backgroundProjection = shellSidebarTabProjection(
            for: tab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_1",
            focusedTabID: "tab_other",
            now: now
        )
        expect(
            backgroundProjection.secondaryLine?.hasPrefix("Command failed 2") == true
                && backgroundProjection.secondaryLine?.contains("Shell") == true,
            "background command failure must remain sidebar-worthy with state first"
        )
    }

    private static func verifiesCommandFailureAcknowledgementSticksAfterFocus() {
        let controller = makeController()
        _ = controller.openTerminalTab()
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let failure = TerminalActivitySnapshot.commandCompletion(exitCode: 2, now: now)

        controller.updateTerminalMetadata(
            metadata(title: "fish", cwd: "/Users/morris/Developer/alan", activity: failure),
            for: "pane_1"
        )

        guard let backgroundTab = controller.shellState.tab(tabID: "tab_main") else {
            fail("test setup must keep the background tab")
        }
        let unacknowledgedProjection = shellSidebarTabProjection(
            for: backgroundTab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_2",
            focusedTabID: "tab_2",
            now: now
        )
        expect(
            unacknowledgedProjection.secondaryLine?.hasPrefix("Command failed 2") == true
                && unacknowledgedProjection.secondaryLine?.contains("Shell") == true,
            "background command failure must remain visible before focus acknowledgement"
        )

        controller.focus(paneID: "pane_1")
        expect(
            controller.pane(paneID: "pane_1")?.activity == nil,
            "focusing the command-failure tab must acknowledge and clear the retained activity"
        )
        expect(
            controller.pane(paneID: "pane_1")?.attention != .notable,
            "acknowledged focused command failure must stop keeping the pane notable"
        )

        controller.focus(paneID: "pane_2")
        guard let acknowledgedTab = controller.shellState.tab(tabID: "tab_main") else {
            fail("test setup must keep the acknowledged tab")
        }
        let acknowledgedProjection = shellSidebarTabProjection(
            for: acknowledgedTab,
            panes: controller.shellState.panes,
            focusedPaneID: "pane_2",
            focusedTabID: "tab_2",
            now: now.addingTimeInterval(1)
        )
        expect(
            acknowledgedProjection.activity == nil,
            "acknowledged command failure must not become sidebar-worthy again after switching away"
        )
        expect(
            acknowledgedProjection.secondaryLine != "Shell · Command failed 2",
            "acknowledged command failure must fall back to tab context instead of resurfacing"
        )
    }

    private static func verifiesCommandFailureAcknowledgementRequiresFocusedPaneInTab() {
        let controller = makeController()
        _ = controller.openTerminalTab()
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let failure = TerminalActivitySnapshot.commandCompletion(exitCode: 2, now: now)

        controller.updateTerminalMetadata(
            metadata(title: "fish", cwd: "/Users/morris/Developer/alan", activity: failure),
            for: "pane_1"
        )
        let mismatchedFocusState = ShellStateSnapshot(
            contractVersion: controller.shellState.contractVersion,
            windowID: controller.shellState.windowID,
            focusedSpaceID: "space_main",
            focusedTabID: "tab_main",
            focusedPaneID: "pane_2",
            spaces: controller.shellState.spaces,
            panes: controller.shellState.panes,
            paneSlots: controller.shellState.paneSlots,
            contents: controller.shellState.contents
        )

        let acknowledged = mismatchedFocusState.acknowledgingCommandFailureActivities(
            in: "tab_main",
            focusedPaneID: "pane_2"
        )

        expect(
            acknowledged.pane(paneID: "pane_1")?.activity != nil,
            "mismatched focus acknowledgement must not clear another tab's command failure"
        )
        expect(
            acknowledged.focusedSpaceID == "space_main"
                && acknowledged.focusedTabID == "tab_main"
                && acknowledged.focusedPaneID == "pane_2",
            "mismatched focus acknowledgement must preserve reducer focus metadata"
        )
    }

    private static func verifiesSpatialFocusAcknowledgesCommandFailure() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        // Focus the right sibling so the left pane's command failure is a background indicator.
        controller.focus(paneID: "pane_2")
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let failure = TerminalActivitySnapshot.commandCompletion(exitCode: 2, now: now)
        controller.updateTerminalMetadata(
            metadata(title: "fish", cwd: "/Users/morris/Developer/alan", activity: failure),
            for: "pane_1"
        )
        expect(
            controller.pane(paneID: "pane_1")?.activity != nil,
            "command failure must be retained on the background sibling before spatial focus"
        )

        let moved = controller.focusAdjacentPane(direction: .left)
        expect(moved, "spatial focus left must move into the left sibling pane")
        expect(
            controller.shellState.focusedPaneID == "pane_1",
            "spatial focus left must focus the left sibling pane"
        )
        expect(
            controller.pane(paneID: "pane_1")?.activity == nil,
            "spatial focus into a command-failure pane must acknowledge and clear its activity"
        )
        expect(
            controller.pane(paneID: "pane_1")?.attention != .notable,
            "acknowledged spatial focus must stop keeping the pane notable"
        )
    }

    private static func verifiesActivityFreshnessPolicies() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let bell = TerminalActivitySnapshot.bellActivity(now: now)
        let exited = TerminalActivitySnapshot.processExitedActivity(exitCode: 2, now: now)
        let needsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed"
        )

        expect(
            bell.isFresh(at: now.addingTimeInterval(7)),
            "bell activity must remain briefly visible"
        )
        expect(
            !bell.isFresh(at: now.addingTimeInterval(9)),
            "bell activity must expire after the brief visibility window"
        )
        expect(
            exited.isFresh(at: now.addingTimeInterval(3_600)),
            "process-exited activity must persist until the pane is closed or replaced"
        )
        expect(
            needsInput.isFresh(at: now.addingTimeInterval(3_600)),
            "needs-input activity must persist until replaced"
        )
    }

    private static func verifiesActivityAttentionIsReadTimeOnly() {
        let projection = ShellPaneProjectionService()
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let failure = activity(
            status: .failed,
            source: .command,
            sourceLabel: "Shell",
            stateLabel: "Command failed 2",
            updatedAt: "2026-05-17T09:00:00Z",
            staleAt: "2026-05-17T09:00:30Z"
        )

        let persistedAttention = projection.projectedAttention(
            metadataAttention: .idle,
            processExited: false,
            binding: nil
        )
        let freshPane = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: persistedAttention,
            activity: failure
        )
        let legacyStalePane = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: .notable,
            activity: failure
        )
        let rendererFailedPane = pane(
            context: context(
                processState: "running",
                rendererHealth: "failed",
                surfaceReadiness: "renderer_failed",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: .notable,
            activity: failure
        )

        expect(persistedAttention == .idle, "activity attention must not be persisted into pane attention")
        expect(
            shellEffectiveAttention(for: freshPane, now: now.addingTimeInterval(10)) == .notable,
            "fresh failed activity may overlay pane attention at read time"
        )
        expect(
            shellEffectiveAttention(for: freshPane, now: now.addingTimeInterval(31)) == .idle,
            "stale failed activity must not overlay pane attention"
        )
        expect(
            shellEffectiveAttention(for: legacyStalePane, now: now.addingTimeInterval(31)) == .idle,
            "legacy stale activity-derived pane attention must be ignored at read time"
        )
        expect(
            shellEffectiveAttention(for: rendererFailedPane, now: now.addingTimeInterval(31)) == .notable,
            "stale activity must not suppress persistent renderer attention"
        )
    }

    private static func verifiesQuietAttentionNeverRequiresUserAction() {
        // Signal-semantics scarcity: only awaiting-user/notable may reach
        // ShellSignal.action in chrome; idle and quiet liveness stay silent.
        for state in ShellAttentionState.allCases {
            let expected = state == .awaitingUser || state == .notable
            expect(
                state.requiresUserAction == expected,
                "attention state \(state.rawValue) must \(expected ? "" : "not ")require user action"
            )
        }

        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let quietLivingPane = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: .active,
            activity: nil
        )
        expect(
            !shellEffectiveAttention(for: quietLivingPane, now: now).requiresUserAction,
            "a quiet living terminal (creation-default .active, no activity) must not project actionable attention"
        )

        let runningCommandPane = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: .active,
            activity: activity(
                status: .running,
                source: .command,
                sourceLabel: "Shell",
                stateLabel: "Running",
                updatedAt: "2026-05-17T09:00:00Z",
                staleAt: "2026-05-17T09:00:30Z"
            )
        )
        expect(
            !shellEffectiveAttention(for: runningCommandPane, now: now.addingTimeInterval(10)).requiresUserAction,
            "a running command must stay out of the action signal"
        )
    }

    private static func verifiesPaneTitleActivityAccessoryLabel() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let paneWithProgress = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: .idle,
            activity: TerminalActivitySnapshot.progressActivity(
                percent: 42,
                now: now
            )
        )

        expect(
            shellPaneActivityAccessoryLabel(for: paneWithProgress, now: now) == "Progress · 42%",
            "pane title activity accessory must expose source-first activity copy"
        )
        expect(
            shellPaneActivityAccessoryLabel(for: paneWithProgress, now: now.addingTimeInterval(16)) == nil,
            "pane title activity accessory must hide stale progress"
        )
    }

    private static func verifiesPaneTitleDetailProjectionIncludesContextBranchAndProcess() {
        let testPane = pane(
            context: context(
                workingDirectoryName: "alan",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "main",
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            cwd: "/Users/morris/Developer/alan",
            process: ShellProcessBinding(program: "fish", argvPreview: nil),
            attention: .idle
        )
        let details = shellPaneTitleBarDetailProjection(
            for: testPane,
            title: "Terminal",
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )

        expect(
            details.map(\.id) == ["worktree", "branch", "process"],
            "pane title details must expose non-redundant worktree, branch, and process"
        )
        expect(details.map(\.title) == ["alan", "main", "fish"], "pane title details must use compact labels")
    }

    private static func verifiesPaneTitleDetailProjectionPreservesResponsivePriority() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let progress = TerminalActivitySnapshot.progressActivity(percent: 42, now: now)
        let testPane = ShellPane(
            paneID: "pane_1",
            tabID: "tab_1",
            spaceID: "space_1",
            launchTarget: .shell,
            cwd: "/Users/morris/Developer/alan",
            process: ShellProcessBinding(program: "fish", argvPreview: nil),
            attention: .notable,
            context: context(
                workingDirectoryName: "alan",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "feature/title-bar",
                processState: "running",
                rendererHealth: "failed",
                surfaceReadiness: "renderer_failed",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            activity: progress,
            alanBinding: ShellAlanBinding(
                processPath: "/proc/1",
                machineState: "running",
                pendingRequest: true,
                source: "test",
                lastProjectedAt: nil
            )
        )

        let details = shellPaneTitleBarDetailProjection(
            for: testPane,
            title: "Editor",
            now: now
        )

        expect(
            details.map(\.id) == ["activity", "status", "worktree", "branch", "process", "alan"],
            "pane title detail projection must preserve responsive priority order"
        )
        expect(
            details.map(\.title) == [
                "Progress · 42%",
                "Renderer failed",
                "alan",
                "feature/title-bar",
                "fish",
                "Input",
            ],
            "pane title detail projection must keep compact labels in priority order"
        )
    }

    private static func verifiesPaneTitleDetailProjectionAvoidsDuplicateAgentAndAlan() {
        let codexActivity = activity(
            status: .running,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Running",
            agent: TerminalActivityAgentMetadata(
                kind: .codex,
                safeSessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            )
        )
        let codexPane = pane(
            context: context(
                workingDirectoryName: "alan",
                repositoryRoot: "/Users/morris/Developer/alan",
                gitBranch: "main",
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            cwd: "/Users/morris/Developer/alan",
            process: ShellProcessBinding(program: "codex", argvPreview: ["codex"]),
            attention: .idle,
            activity: codexActivity
        )
        let codexDetails = shellPaneTitleBarDetailProjection(
            for: codexPane,
            title: "alan",
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )
        expect(codexDetails.map(\.id) == ["activity", "branch"], "Codex activity must not duplicate process")

        let alanActivity = activity(
            status: .running,
            source: .alan,
            sourceLabel: "alan",
            stateLabel: "Running"
        )
        let alanPane = ShellPane(
            paneID: "pane_1",
            tabID: "tab_1",
            spaceID: "space_1",
            launchTarget: .shell,
            cwd: "/Users/morris/Developer/alan",
            process: ShellProcessBinding(program: "alan", argvPreview: ["alan", "chat"]),
            attention: .active,
            context: context(
                workingDirectoryName: "alan",
                repositoryRoot: nil,
                gitBranch: nil,
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            activity: alanActivity,
            alanBinding: ShellAlanBinding(
                processPath: "/proc/1",
                machineState: "running",
                pendingRequest: false,
                source: "test",
                lastProjectedAt: nil
            )
        )
        let alanDetails = shellPaneTitleBarDetailProjection(
            for: alanPane,
            title: "alan",
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )
        expect(alanDetails.map(\.id) == ["activity"], "alan activity must not duplicate alan binding or process")
    }

    private static func verifiesActivityNotificationPolicyIsLowNoise() {
        let now = Date(timeIntervalSince1970: 1_779_008_400)
        let testPane = pane(
            context: context(
                processState: "running",
                rendererHealth: "ready",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            viewport: nil,
            attention: .idle
        )
        let testTab = ShellTab(
            tabID: "tab_1",
            kind: .terminal,
            title: "alan",
            paneTree: ShellPaneTreeNode(
                nodeID: "node_pane_1",
                kind: .pane,
                direction: nil,
                paneID: "pane_1",
                children: nil
            )
        )

        let focusedProgress = TerminalActivitySnapshot.progressActivity(percent: 42, now: now)
        expect(
            shellActivityNotificationRoute(
                for: focusedProgress,
                pane: testPane,
                tab: testTab,
                visibility: .focusedVisible,
                now: now
            ) == nil,
            "focused progress must stay visual-only"
        )

        let agentNeedsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed",
            agent: .init(
                kind: .codex,
                safeSessionLabel: "codex",
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            )
        )
        let needsInputRoute = shellActivityNotificationRoute(
            for: agentNeedsInput,
            pane: testPane,
            tab: testTab,
            visibility: .background,
            now: now
        )
        expect(needsInputRoute?.kind == .needsInput, "background agent input must be notification-worthy")
        expect(needsInputRoute?.attention == .awaitingUser, "agent input must mark tab as awaiting user")

        let focusedSuccess = commandActivity(
            exitCode: 0,
            durationMilliseconds: 120_000,
            updatedAt: "2026-05-17T09:00:00Z"
        )
        expect(
            shellActivityNotificationRoute(
                for: focusedSuccess,
                pane: testPane,
                tab: testTab,
                visibility: .focusedVisible,
                now: now
            ) == nil,
            "focused command success must not send a notification"
        )

        let shortBackgroundSuccess = commandActivity(
            exitCode: 0,
            durationMilliseconds: 5_000,
            updatedAt: "2026-05-17T09:00:00Z"
        )
        expect(
            shellActivityNotificationRoute(
                for: shortBackgroundSuccess,
                pane: testPane,
                tab: testTab,
                visibility: .background,
                now: now
            ) == nil,
            "short background command success must remain quiet"
        )

        let longBackgroundSuccess = commandActivity(
            exitCode: 0,
            durationMilliseconds: 120_000,
            updatedAt: "2026-05-17T09:00:00Z"
        )
        let longCommandRoute = shellActivityNotificationRoute(
            for: longBackgroundSuccess,
            pane: testPane,
            tab: testTab,
            visibility: .background,
            now: now
        )
        expect(longCommandRoute?.kind == .commandCompleted, "long background command completion must route")
        expect(longCommandRoute?.attention == .notable, "long command completion must mark the tab notable")

        let realFactoryLongSuccess = TerminalActivitySnapshot.commandCompletion(
            exitCode: 0,
            now: now,
            durationMilliseconds: 120_000
        )
        expect(
            shellActivityNotificationRoute(
                for: realFactoryLongSuccess,
                pane: testPane,
                tab: testTab,
                visibility: .background,
                now: now
            )?.kind == .commandCompleted,
            "factory-produced long command completion must route"
        )

        let exited = TerminalActivitySnapshot.processExitedActivity(exitCode: 9, now: now)
        let exitedRoute = shellActivityNotificationRoute(
            for: exited,
            pane: testPane,
            tab: testTab,
            visibility: .background,
            now: now
        )
        expect(exitedRoute?.kind == .processExited, "background process exit must route")
        expect(exitedRoute?.attention == .awaitingUser, "process exit must mark the tab awaiting user")
    }

    private static func verifiesControllerRoutesActivityNotificationsOnce() {
        let controller = makeController()
        _ = controller.openTerminalTab()
        guard let backgroundPane = controller.shellState.panes.first(where: { $0.paneID != "pane_1" }) else {
            fail("test setup must create a background pane")
        }
        controller.focus(paneID: "pane_1")

        let needsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed",
            agent: .init(
                kind: .codex,
                safeSessionLabel: "codex",
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            )
        )
        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: needsInput),
            for: backgroundPane.paneID
        )
        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: needsInput),
            for: backgroundPane.paneID
        )

        expect(
            controller.activityNotifications.count == 1,
            "controller must route one notification per activity update"
        )
        expect(
            controller.activityNotifications.first?.kind == .needsInput,
            "controller notification must preserve the routed activity kind"
        )
        expect(
            controller.shellState.pane(paneID: backgroundPane.paneID)?.attention == .idle,
            "notification-worthy activity must not persist into pane attention"
        )
        expect(
            controller.shellState.pane(paneID: backgroundPane.paneID).map {
                shellEffectiveAttention(for: $0, now: Date())
            } == .awaitingUser,
            "notification-worthy agent input must overlay its pane awaiting user at read time"
        )
    }

    private static func verifiesControllerRoutesDistinctActivityPayloadsInSameSecond() {
        let controller = makeController()
        _ = controller.openTerminalTab()
        guard let backgroundPane = controller.shellState.panes.first(where: { $0.paneID != "pane_1" }) else {
            fail("test setup must create a background pane")
        }
        controller.focus(paneID: "pane_1")

        let firstNeedsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed",
            detailLabel: "Review plan",
            agent: .init(
                kind: .codex,
                safeSessionLabel: "codex",
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            ),
            updatedAt: "2026-05-17T09:00:00Z"
        )
        let secondNeedsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed",
            detailLabel: "Approve changes",
            agent: .init(
                kind: .codex,
                safeSessionLabel: "codex",
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            ),
            updatedAt: "2026-05-17T09:00:00Z"
        )

        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: firstNeedsInput),
            for: backgroundPane.paneID
        )
        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: secondNeedsInput),
            for: backgroundPane.paneID
        )

        expect(
            controller.activityNotifications.count == 2,
            "distinct same-second activity payloads must each route a notification"
        )
        expect(
            controller.activityNotifications.first?.id != controller.activityNotifications.last?.id,
            "notification ids must include a payload discriminator beyond the second-level timestamp"
        )
    }

    private static func verifiesInactiveAppRoutesFocusedPaneNotifications() {
        let controller = makeController(appIsActive: false)
        let needsInput = activity(
            status: .needsInput,
            source: .codex,
            sourceLabel: "Codex",
            stateLabel: "Input needed",
            agent: .init(
                kind: .codex,
                safeSessionLabel: "codex",
                projectLabel: "alan",
                workingDirectory: "/Users/morris/Developer/alan"
            )
        )

        controller.updateTerminalMetadata(
            metadata(title: "codex", cwd: "/repo/app", activity: needsInput),
            for: "pane_1"
        )

        expect(
            controller.activityNotifications.count == 1,
            "inactive app must route focused pane activity because it is out of view"
        )
        expect(
            controller.activityNotifications.first?.kind == .needsInput,
            "inactive focused pane notification must preserve the routed activity kind"
        )
    }


    private static func verifiesProcessExitNotificationRoutesBeforeAutoClose() {
        let controller = makeController()
        _ = controller.openTerminalTab(workingDirectory: "/second")
        controller.focus(paneID: "pane_1")

        let processExitActivity = TerminalActivitySnapshot.processExitedActivity(
            exitCode: 0,
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )

        controller.updateTerminalMetadata(
            childExitMetadata(title: "fish", exitCode: 0, activity: processExitActivity),
            for: "pane_2"
        )

        expect(controller.pane(paneID: "pane_2") == nil, "child exit must still close the pane")
        expect(
            controller.activityNotifications.count == 1,
            "process-exit activity must route before auto-close removes the pane"
        )
        expect(
            controller.activityNotifications.first?.kind == .processExited,
            "auto-closed process exit must preserve the notification kind"
        )
        expect(
            controller.activityNotifications.first?.paneID == "pane_2",
            "auto-closed process exit notification must point at the exiting pane"
        )
    }

    private static func verifiesProcessExitRuntimeNotificationRoutesBeforeAutoClose() {
        let controller = makeController()
        _ = controller.openTerminalTab(workingDirectory: "/second")
        controller.focus(paneID: "pane_1")
        guard let exitingPane = controller.pane(paneID: "pane_2") else {
            fail("test setup must create a background pane")
        }

        let processExitActivity = TerminalActivitySnapshot.processExitedActivity(
            exitCode: 130,
            now: Date(timeIntervalSince1970: 1_779_008_400)
        )

        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: exitingPane.terminalContentID,
                paneID: exitingPane.paneID,
                tabID: exitingPane.tabID,
                logicalSize: .zero,
                backingSize: .zero,
                displayName: "Studio Display",
                displayID: "display_1",
                attachedWindowTitle: "alan",
                isFocused: false,
                renderer: TerminalRendererSnapshot(
                    kind: .ghosttyLive,
                    phase: .surfaceReady,
                    summary: "surface ready",
                    detail: nil,
                    failureReason: nil,
                    recentEvents: []
                ),
                paneMetadata: childExitMetadata(
                    title: "fish",
                    exitCode: 130,
                    activity: processExitActivity
                ),
                surfaceState: AlanTerminalSurfaceStateSnapshot(
                    readiness: .unready(reason: .childExited),
                    terminalMode: .normalBuffer,
                    scrollback: .empty,
                    search: nil,
                    semanticCommands: .placeholder,
                    readonly: false,
                    secureInput: false,
                    inputReady: false,
                    rendererHealth: "surface_ready",
                    childExited: true,
                    lastUpdatedAt: Date(timeIntervalSince1970: 2_001)
                ),
                lastUpdatedAt: Date(timeIntervalSince1970: 2_002)
            )
        )

        expect(controller.pane(paneID: "pane_2") == nil, "runtime child exit must still close the pane")
        expect(
            controller.activityNotifications.count == 1,
            "runtime process-exit activity must route before auto-close removes the pane"
        )
        expect(
            controller.activityNotifications.first?.kind == .processExited,
            "runtime auto-closed process exit must preserve the notification kind"
        )
    }

    private static func verifiesTerminalChildExitIgnoresStaleForegroundContext() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)

        controller.updateTerminalMetadata(
            metadata(title: "make test", activeTaskState: .foregroundCommand),
            for: "pane_2"
        )
        expect(
            controller.pane(paneID: "pane_2")?.context?.processState == "foreground_command",
            "test setup must project foreground command state before child exit"
        )
        expect(
            controller.closeGuardImpact(for: .paneSlot("pane_2"))?.requiresConfirmation == true,
            "foreground command pane must still be protected for user-initiated closes"
        )

        controller.updateTerminalMetadata(childExitMetadata(title: "make test", exitCode: 0), for: "pane_2")

        expect(
            controller.pane(paneID: "pane_2") == nil,
            "child exit cleanup must bypass stale foreground close guard state"
        )
        expect(controller.pane(paneID: "pane_1") != nil, "child exit must preserve sibling panes")
    }

    private static func verifiesRuntimeExitCloseBypassesStaleActiveGuard() {
        let presenter = FakeShellCloseConfirmationPresenter(nextResponses: [false])
        let controller = makeController(closeConfirmationPresenter: presenter)
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)

        controller.updateTerminalMetadata(
            metadata(title: "make test", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        expect(
            controller.closeGuardImpact(for: .paneSlot("pane_1"))?.requiresConfirmation == true,
            "test setup must project stale foreground close-guard state"
        )

        expect(
            controller.closePaneAfterTerminalRuntimeExit(paneID: "pane_1"),
            "runtime-exit close must bypass active-work confirmation"
        )

        expect(presenter.impacts.isEmpty, "runtime-exit close must not present active-work confirmation")
        expect(controller.pane(paneID: "pane_1") == nil, "runtime-exit close must remove the pane")
        expect(handle.teardownCount == 1, "runtime-exit close must finalize the terminal runtime")
    }

    private static func verifiesTerminalChildExitClosesSplitPane() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)

        controller.updateTerminalMetadata(childExitMetadata(title: "fish", exitCode: 0), for: "pane_2")

        expect(controller.pane(paneID: "pane_2") == nil, "child exit must close the owning split pane")
        expect(controller.pane(paneID: "pane_1") != nil, "child exit must preserve sibling panes")
        expect(controller.shellState.focusedPaneID == "pane_1", "child exit must focus the remaining sibling")
    }

    private static func verifiesTerminalChildExitClosesSinglePaneTab() {
        let controller = makeController()
        _ = controller.openTerminalTab(workingDirectory: "/second")

        controller.updateTerminalMetadata(childExitMetadata(title: "fish", exitCode: 0), for: "pane_2")

        expect(controller.shellState.tab(tabID: "tab_2") == nil, "child exit must close the owning single-pane tab")
        expect(controller.pane(paneID: "pane_1") != nil, "child exit must preserve other tabs")
        expect(controller.shellState.focusedPaneID == "pane_1", "child exit must move focus to the next valid pane")
    }

    private static func verifiesTerminalChildExitCanLeaveEmptyFocusedSpace() {
        let controller = makeController()

        controller.updateTerminalMetadata(childExitMetadata(title: "fish", exitCode: 0), for: "pane_1")

        expect(controller.shellState.spaces.count == 1, "final child exit must keep the focused space")
        expect(controller.shellState.spaces.first?.tabs.isEmpty == true, "final child exit may leave an empty space")
        expect(controller.shellState.panes.isEmpty, "final child exit must not restart a replacement pane")
        expect(controller.shellState.focusedPaneID == nil, "final child exit must clear focused pane")
    }

    private static func verifiesClosingTabReleasesTerminalRuntime() {
        let controller = makeController()
        guard let pane = controller.selectedPane else {
            fail("test setup must expose selected pane")
        }
        _ = controller.terminalRuntimeRegistry.surfaceHandle(for: pane, bootProfile: nil)

        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.contains(pane.paneID),
            "test setup must register selected pane runtime"
        )

        _ = controller.closeTab(tabID: pane.tabID)

        expect(
            !controller.terminalRuntimeRegistry.registeredPaneIDs.contains(pane.paneID),
            "closing a tab must release its terminal runtime through the registry"
        )
    }

    private static func verifiesTabSelectionCommitsAuthoritativeFocus() {
        let controller = makeController()
        _ = controller.openTerminalTab()
        controller.focus(paneID: "pane_1")

        guard let targetPane = controller.pane(paneID: "pane_2") else {
            fail("test setup must create second tab pane")
        }
        let targetHostView = controller.terminalRuntimeRegistry.hostView(
            for: targetPane,
            bootProfile: controller.bootProfile(for: targetPane),
            isSelected: false,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )

        controller.select(tabID: "tab_2")
        controller.updateTerminalMetadata(
            metadata(title: "old focused pane updated"),
            for: "pane_1"
        )

        expect(
            controller.shellState.focusedPaneID == "pane_2",
            "tab selection must update authoritative focused pane"
        )
        expect(controller.selectedTabID == "tab_2", "runtime metadata must not revert selected tab")
        expect(controller.selectedPane?.paneID == "pane_2", "selected pane must follow selected tab focus")
        expect(
            targetHostView.focusCount == 1,
            "tab selection must request focus for the target terminal runtime"
        )
    }

    private static func verifiesShellActionTabNavigationTargetsCurrentSelection() {
        let controller = makeController()
        _ = controller.openTerminalTab()
        _ = controller.openTerminalTab()

        let result = controller.performShellAction(.tabSelectPrevious, target: .contextTab("tab_main"))

        expect(result == .executed, "previous-tab shortcut action must execute with multiple tabs")
        expect(
            controller.selectedTabID == "tab_2",
            "keyboard tab navigation must use the current selected tab, not a context-menu tab target"
        )
        expect(
            controller.shellState.focusedPaneID == "pane_2",
            "keyboard tab navigation must commit focus for the selected tab"
        )
    }

    private static func verifiesSpaceSelectionCommitsAuthoritativeFocus() {
        let controller = makeController()
        _ = controller.createTerminalSpace(title: "Second", workingDirectory: "/tmp")
        controller.focus(paneID: "pane_1")

        guard let targetPane = controller.pane(paneID: "pane_2") else {
            fail("test setup must create second space pane")
        }
        let targetHostView = controller.terminalRuntimeRegistry.hostView(
            for: targetPane,
            bootProfile: controller.bootProfile(for: targetPane),
            isSelected: false,
            activationDelegate: nil,
            onShellAction: nil,
            onCloseRequest: nil,
            onRuntimeUpdate: { _ in },
            onMetadataUpdate: { _ in }
        )

        controller.select(spaceID: "space_2")
        controller.updateTerminalMetadata(
            metadata(title: "old focused pane updated"),
            for: "pane_1"
        )

        expect(
            controller.shellState.focusedSpaceID == "space_2",
            "space selection must update focused space"
        )
        expect(controller.shellState.focusedTabID == "tab_2", "space selection must update focused tab")
        expect(
            controller.shellState.focusedPaneID == "pane_2",
            "space selection must update authoritative focused pane"
        )
        expect(controller.selectedSpaceID == "space_2", "runtime metadata must not revert selected space")
        expect(controller.selectedTabID == "tab_2", "runtime metadata must not revert selected space tab")
        expect(
            targetHostView.focusCount == 1,
            "space selection must request focus for the target terminal runtime"
        )
    }

    private static func verifiesSpaceSelectionRestoresRememberedTab() {
        let controller = makeController()
        _ = controller.openTerminalTab(title: "Second")

        expect(
            controller.selectedSpaceID == "space_main",
            "test setup must start in the main space"
        )
        expect(
            controller.selectedTabID == "tab_2",
            "test setup must select the second tab in the main space"
        )

        _ = controller.createTerminalSpace(title: "Other", workingDirectory: "/tmp")
        expect(controller.selectedSpaceID == "space_2", "test setup must switch to the second space")

        controller.select(spaceID: "space_main")

        expect(controller.selectedSpaceID == "space_main", "space selection must return to the main space")
        expect(
            controller.selectedTabID == "tab_2",
            "returning to a space must restore its remembered selected tab instead of the first tab"
        )
        expect(
            controller.shellState.focusedPaneID == "pane_2",
            "restored space tab selection must commit authoritative pane focus"
        )
    }

    private static func verifiesShellActionSpaceSelectionReportsMissingTargets() {
        let controller = makeController()
        let selectedSpaceBefore = controller.selectedSpaceID

        let result = controller.performShellAction(.spaceSelectByIndex, target: .spaceIndex(8))

        expect(
            result == .unavailable(reason: "Space is not available"),
            "missing numeric space shortcuts must report a stable unavailable reason"
        )
        expect(
            controller.selectedSpaceID == selectedSpaceBefore,
            "missing numeric space shortcuts must not change the selected space"
        )
    }

    private static func verifiesWorkspaceManifestRestoresInactiveSpaceSelection() {
        let windowID = "space_selection_restore_\(UUID().uuidString)"
        let url = manifestURL("space-selection-restore")
        let store = ShellWorkspaceManifestStore(manifestURL: url)
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 120)
            )
        )

        _ = controller.openTerminalTab(title: "Main Second")
        _ = controller.createTerminalSpace(title: "Second Space", workingDirectory: "/tmp")

        guard let savedManifest = decodeManifest(at: url) else {
            fail("space selection changes must persist workspace manifest")
        }
        expect(
            savedManifest.spaces.first { $0.spaceID == "space_main" }?.selectedTabID == "tab_2",
            "manifest must remember the selected tab for an inactive space"
        )
        expect(
            savedManifest.spaces.first { $0.spaceID == "space_2" }?.selectedTabID == "tab_3",
            "manifest must remember the selected tab for the active space"
        )
        expect(savedManifest.selectedSpaceID == "space_2", "manifest must keep active restart space")
        expect(savedManifest.selectedTabID == "tab_3", "manifest must keep active restart tab")

        let restoredContext = ShellWindowContext.make(
            windowID: windowID,
            terminalRuntimeRegistry: TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        )
        let restored = ShellHostController.live(
            windowContext: restoredContext,
            workspaceManifestURL: url,
            defaultWorkingDirectory: "/tmp",
            now: Date(timeIntervalSince1970: 121)
        )

        expect(restored.selectedSpaceID == "space_2", "restart must restore globally selected space")
        expect(restored.selectedTabID == "tab_3", "restart must restore globally selected tab")

        restored.select(spaceID: "space_main")

        expect(restored.selectedSpaceID == "space_main", "restored controller must switch to inactive space")
        expect(
            restored.selectedTabID == "tab_2",
            "restored inactive space must use its remembered tab instead of the first tab"
        )
        expect(
            restored.shellState.focusedPaneID == "pane_2",
            "restored inactive space tab selection must commit pane focus"
        )
    }

    private static func verifiesSplitTabSelectionUsesStablePaneWithoutChangingLayout() {
        let controller = makeController()
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        let splitTreeBefore = controller.shellState.tab(tabID: "tab_main")?.paneTree
        _ = controller.openTerminalTab()

        controller.select(tabID: "tab_main")

        let splitTreeAfter = controller.shellState.tab(tabID: "tab_main")?.paneTree
        expect(
            controller.shellState.focusedPaneID == "pane_1",
            "split-tab selection must choose a stable pane from the tab tree"
        )
        expect(
            splitTreeAfter == splitTreeBefore,
            "split-tab selection must not rewrite split tree or divider ratios"
        )
    }

    private static func verifiesContentStateProjectionSeparatesPaneSlotsAndContent() {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        do {
            state = try state.splittingPane("pane_1", placement: .right).state
        } catch {
            fail("content projection setup must split the bootstrap pane: \(error)")
        }

        let projection = state.contentStateProjection()
        let tab = projection.tab(tabID: "tab_main")

        expect(
            projection.contractVersion == ShellContentStateSnapshot.currentContractVersion,
            "content projection must publish the v0.2 contract"
        )
        expect(
            projection.focusedPaneSlotID == state.focusedPaneID,
            "content projection must map focused pane to focused pane slot"
        )
        expect(
            tab?.paneTree.paneSlotIDs == ["pane_1", "pane_2"],
            "content projection layout leaves must reference pane_slot_id values"
        )
        expect(
            projection.paneSlots.map(\.contentID) == ["content_pane_1", "content_pane_2"],
            "content projection must bind each PaneSlot to a distinct ContentInstance"
        )
        expect(
            projection.contents.map(\.kind) == [.terminal, .terminal],
            "terminal-only runtime panes must project as terminal content instances"
        )
        expect(
            tab.flatMap { projection.userFacingTitle(for: $0) } == "Shell",
            "content-aware title projection must derive sidebar titles from tab/content descriptors"
        )

        let emptySpaceState = ShellStateSnapshot(
            contractVersion: "0.1",
            windowID: "empty_projection",
            focusedSpaceID: "space_empty",
            focusedTabID: nil,
            focusedPaneID: nil,
            spaces: [
                ShellSpace(
                    spaceID: "space_empty",
                    title: "Empty",
                    attention: .idle,
                    tabs: []
                )
            ],
            panes: []
        )
        let emptyProjection = emptySpaceState.contentStateProjection()
        expect(emptyProjection.focusedPaneSlotID == nil, "empty content projection must keep pane-slot focus nil")
        expect(emptyProjection.paneSlots.isEmpty, "empty content projection must not fabricate PaneSlots")
        expect(emptyProjection.spaces.first?.spaceID == "space_empty", "empty content projection must keep spaces")
    }

    private static func verifiesContentRenderingRegistryRoutesSupportedKinds() {
        let terminal = ShellContentInstance(
            contentID: "content_terminal",
            kind: .terminal,
            title: "Shell",
            payload: .terminal(
                ShellTerminalContentPayload(
                    launchTarget: .shell,
                    cwd: "/tmp",
                    title: "Shell"
                )
            )
        )
        let markdown = ShellContentInstance(
            contentID: "content_markdown",
            kind: .markdown,
            title: "README.md",
            payload: .markdown(
                ShellMarkdownContentPayload(
                    fileURL: "file:///tmp/README.md",
                    title: "README.md"
                )
            )
        )
        let settings = ShellContentInstance(
            contentID: "content_settings",
            kind: .settings,
            title: "Settings",
            payload: .settings(
                ShellSettingsContentPayload(
                    surfaceID: "settings_main",
                    title: "Settings"
                )
            )
        )

        let terminalDescriptor = ShellContentRenderingRegistry.descriptor(for: terminal)
        expect(terminalDescriptor.renderKind == .terminal, "terminal content must route to terminal renderer")
        expect(terminalDescriptor.isTerminalSurface, "terminal descriptor must expose terminal ownership")
        expect(
            terminalDescriptor.capabilities.contains(.terminalInput),
            "terminal descriptor must retain terminal input capability"
        )

        let markdownDescriptor = ShellContentRenderingRegistry.descriptor(for: markdown)
        expect(markdownDescriptor.renderKind == .markdown, "markdown content must route to markdown renderer")
        expect(markdownDescriptor.iconName == "doc.text", "markdown descriptor must get a bounded viewer icon")
        expect(
            !markdownDescriptor.capabilities.contains(.terminalInput),
            "markdown descriptor must not expose terminal input capability"
        )

        let settingsDescriptor = ShellContentRenderingRegistry.descriptor(for: settings)
        expect(settingsDescriptor.renderKind == .settings, "settings content must route to settings renderer")
        expect(settingsDescriptor.iconName == "gearshape", "settings descriptor must get a settings icon")
        expect(
            settingsDescriptor.capabilities == [.settingsSurface],
            "settings descriptor must retain settings capabilities"
        )

        let missingDescriptor = ShellContentRenderingRegistry.descriptor(for: nil)
        expect(missingDescriptor.renderKind == .unavailable, "missing content must route to bounded fallback")
        expect(
            missingDescriptor.contentID == nil,
            "missing content fallback must not invent a content identity"
        )

    }

    private static func verifiesContentAwareSidebarProjectionUsesNonTerminalLabels() {
        let controller = makeController()
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(
                fileURL: FileManager.default.temporaryDirectory
                    .appendingPathComponent("sidebar-notes.md"),
                title: "Research Notes"
            )
        ) else {
            fail("sidebar projection setup must create a markdown split")
        }

        guard let tab = controller.shellState.tab(tabID: "tab_main") else {
            fail("sidebar projection setup must keep the mixed tab")
        }
        let projection = shellSidebarTabProjection(
            for: tab,
            panes: controller.shellState.panes,
            contentState: controller.shellState.contentStateProjection(),
            focusedPaneID: markdownPaneID,
            focusedTabID: tab.tabID
        )
        expect(
            projection.title == "Research Notes",
            "focused markdown content must provide the sidebar row primary label"
        )
        expect(
            projection.secondaryLine == "Document",
            "focused markdown content must provide a user-facing type hint"
        )
        expect(
            !projection.title.contains("pane_")
                && projection.secondaryLine?.contains("content_") != true,
            "content-aware sidebar labels must not expose implementation identifiers"
        )

        _ = controller.openSettingsTab()
        guard let settingsTab = controller.selectedTab else {
            fail("settings open must focus a settings tab")
        }
        let settingsProjection = shellSidebarTabProjection(
            for: settingsTab,
            panes: controller.shellState.panes,
            contentState: controller.shellState.contentStateProjection(),
            focusedPaneID: controller.shellState.focusedPaneID,
            focusedTabID: settingsTab.tabID
        )
        expect(settingsProjection.title == "Settings", "settings content must label the sidebar row")
        expect(settingsProjection.secondaryLine == "Settings", "settings content must expose a type hint")
    }

    private static func verifiesOpeningMarkdownTabCreatesReadOnlyContentDescriptor() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("Alan-\(UUID().uuidString).md")
        let markdown = "# Notes\n\nRead-only content."
        do {
            try markdown.write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("markdown open setup must create a file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let controller = makeController()
        guard let tabID = controller.openMarkdownTab(fileURL: fileURL) else {
            fail("opening markdown must create a shell tab")
        }

        let projection = controller.shellState.contentStateProjection()
        guard let content = projection.focusedContent else {
            fail("markdown tab must focus a content descriptor")
        }
        let descriptor = ShellContentRenderingRegistry.descriptor(for: content)
        let expectedURL = fileURL.standardizedFileURL.absoluteString

        expect(controller.shellState.focusedTabID == tabID, "markdown open must focus the new tab")
        expect(content.kind == .markdown, "markdown open must create markdown content")
        expect(content.title == fileURL.lastPathComponent, "markdown title must come from file name")
        expect(
            content.payload.markdown?.fileURL == expectedURL,
            "markdown descriptor must persist the backing file URL"
        )
        expect(
            content.capabilities == [.markdownReadOnlyViewer],
            "markdown descriptor must expose only read-only viewer capability"
        )
        expect(
            !content.capabilities.contains(.terminalInput),
            "markdown descriptor must not expose terminal input"
        )
        expect(descriptor.renderKind == .markdown, "markdown descriptor must route to markdown renderer")
        expect(
            descriptor.payload?.markdown?.fileURL == expectedURL,
            "render descriptor must carry the markdown file payload"
        )
        expect(
            controller.selectedPane?.launchTarget == nil && controller.selectedPane?.process == nil,
            "markdown pane must not describe a terminal process"
        )
        expect(
            controller.selectedPane.map {
                !controller.terminalRuntimeRegistry.registeredPaneIDs.contains($0.paneID)
            } == true,
            "markdown open must not create a terminal runtime"
        )

    }

    private static func verifiesOpeningContentTabDefaultsToTerminalIntent() {
        let controller = makeController()
        let initialTabCount = controller.shellState.spaces.flatMap(\.tabs).count
        guard let tabID = controller.openContentTab() else {
            fail("default content-intent tab open must create a shell tab")
        }

        let projection = controller.shellState.contentStateProjection()
        let content = projection.focusedContent
        let tab = controller.shellState.tab(tabID: tabID)

        expect(tab?.title == "Shell 2", "default content-intent tab must preserve terminal tab title behavior")
        expect(
            controller.shellState.spaces.flatMap(\.tabs).count == initialTabCount + 1,
            "default content-intent tab must append one tab"
        )
        expect(content?.kind == .terminal, "default content-intent tab must create terminal content")
        expect(
            controller.selectedPane?.launchTarget == .shell,
            "default content-intent tab must keep New Terminal Tab behavior"
        )
    }

    private static func verifiesOpeningSettingsTabCreatesSingletonShellContent() {
        let controller = makeController()
        let initialTabCount = controller.shellState.spaces.flatMap(\.tabs).count
        guard let firstTabID = controller.openSettingsTab() else {
            fail("opening settings must create a shell tab")
        }

        let firstProjection = controller.shellState.contentStateProjection()
        guard let content = firstProjection.focusedContent else {
            fail("settings tab must focus a content descriptor")
        }
        let descriptor = ShellContentRenderingRegistry.descriptor(for: content)
        let selectedPaneID = controller.selectedPane?.paneID

        expect(controller.shellState.focusedTabID == firstTabID, "settings open must focus the settings tab")
        expect(content.contentID == ShellContentInstance.settingsContentID, "settings content must use the canonical content ID")
        expect(content.kind == .settings, "settings open must create settings content")
        expect(content.title == "Settings", "settings descriptor must expose a user-facing title")
        expect(
            content.payload.settings?.surfaceID == ShellContentInstance.settingsSurfaceID,
            "settings descriptor must persist the canonical settings surface"
        )
        expect(
            content.capabilities == [.settingsSurface],
            "settings descriptor must expose only settings surface capability"
        )
        expect(
            !content.capabilities.contains(.terminalInput),
            "settings descriptor must not expose terminal input"
        )
        expect(descriptor.renderKind == .settings, "settings descriptor must route to settings renderer")
        expect(
            controller.selectedPane?.launchTarget == nil && controller.selectedPane?.process == nil,
            "settings pane must not describe a terminal process"
        )
        expect(
            selectedPaneID.map {
                !controller.terminalRuntimeRegistry.registeredPaneIDs.contains($0)
            } == true,
            "settings open must not create a terminal runtime"
        )

        guard let secondTabID = controller.openSettingsTab() else {
            fail("reopening settings must focus the existing settings tab")
        }
        let secondProjection = controller.shellState.contentStateProjection()
        let settingsContents = secondProjection.contents.filter { $0.kind == .settings }
        let settingsSlots = secondProjection.paneSlots.filter { paneSlot in
            secondProjection.content(contentID: paneSlot.contentID)?.kind == .settings
        }

        expect(secondTabID == firstTabID, "reopening settings must return the existing tab")
        expect(controller.shellState.focusedTabID == firstTabID, "reopening settings must keep focus on the settings tab")
        expect(
            controller.shellState.spaces.flatMap(\.tabs).count == initialTabCount + 1,
            "reopening settings must not create duplicate tabs"
        )
        expect(settingsContents.count == 1, "settings content must remain singleton")
        expect(settingsSlots.count == 1, "settings PaneSlot must remain singleton")

    }

    private static func verifiesSplitPaneAcceptsMarkdownContentIntent() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("Split-\(UUID().uuidString).md")
        do {
            try "## Split notes\n".write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("markdown split setup must create a file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let controller = makeController()
        let terminalContentIDBefore = controller.shellState
            .contentStateProjection()
            .contentMounted(in: "pane_1")?
            .contentID
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(fileURL: fileURL, title: nil)
        ) else {
            fail("markdown content intent must create a split pane")
        }

        let projection = controller.shellState.contentStateProjection()
        let markdownContent = projection.contentMounted(in: markdownPaneID)
        let terminalContentIDAfter = projection.contentMounted(in: "pane_1")?.contentID
        let paneTree = projection.tab(tabID: "tab_main")?.paneTree

        expect(
            paneTree?.paneSlotIDs == ["pane_1", markdownPaneID],
            "markdown split intent must add a PaneSlot beside the existing terminal"
        )
        expect(markdownContent?.kind == .markdown, "markdown split intent must mount markdown content")
        expect(
            markdownContent?.payload.markdown?.fileURL == fileURL.standardizedFileURL.absoluteString,
            "markdown split intent must persist the markdown file URL"
        )
        expect(
            terminalContentIDAfter == terminalContentIDBefore,
            "markdown split intent must preserve existing terminal content identity"
        )
        expect(
            controller.selectedPane?.launchTarget == nil && controller.selectedPane?.process == nil,
            "markdown split intent must not create a terminal process for the new PaneSlot"
        )
    }

    private static func verifiesControlPlaneResponsesExposeContentContainers() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("ControlPlane-\(UUID().uuidString).md")
        do {
            try "## Control plane notes\n".write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("control-plane content setup must create a markdown file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let controller = makeController()
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(fileURL: fileURL, title: "Notes")
        ) else {
            fail("control-plane setup must create markdown content")
        }
        guard let settingsPaneID = controller.splitPane(
            paneID: markdownPaneID,
            placement: .down,
            contentIntent: .settings(title: "Settings")
        ) else {
            fail("control-plane setup must create settings content")
        }
        controller.focus(paneID: settingsPaneID)

        let stateResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "content-state-1",
                  "command": "state"
                }
                """
            )
        )
        let statePaneSlotIDs = Set(stateResponse.paneSlots?.map(\.paneSlotID) ?? [])
        let stateContentKinds = Set(stateResponse.contents?.map(\.kind) ?? [])
        expect(
            stateResponse.contractVersion == ShellContentStateSnapshot.currentContractVersion,
            "control-plane state response must advertise the content-state contract"
        )
        expect(
            statePaneSlotIDs.isSuperset(of: Set(["pane_1", markdownPaneID, settingsPaneID])),
            "control-plane state response must expose PaneSlot descriptors"
        )
        expect(
            stateContentKinds == [.terminal, .markdown, .settings],
            "control-plane state response must expose mounted ContentInstances"
        )
        expect(
            stateResponse.focusedPaneSlotID == settingsPaneID
                && stateResponse.paneSlotID == settingsPaneID,
            "control-plane state response must report focused PaneSlot identity"
        )
        expect(
            stateResponse.contentKind == .settings
                && stateResponse.contentCapabilities == [.settingsSurface],
            "control-plane state response must expose focused content capabilities"
        )

        let snapshotResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "content-snapshot-1",
                  "command": "pane.snapshot",
                  "pane_id": "\(markdownPaneID)"
                }
                """
            )
        )
        expect(
            snapshotResponse.paneSlotID == markdownPaneID
                && snapshotResponse.contentKind == .markdown
                && snapshotResponse.contentTitle == "Notes",
            "pane.snapshot response must project the PaneSlot's mounted content"
        )
        expect(
            snapshotResponse.contentCapabilities == [.markdownReadOnlyViewer],
            "pane.snapshot response must expose content capabilities"
        )

        let listResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "content-list-1",
                  "command": "pane.list",
                  "tab_id": "\(controller.selectedTabID ?? "")"
                }
                """
            )
        )
        expect(
            listResponse.paneSlots?.count == 3 && listResponse.contents?.count == 3,
            "pane.list response must include tab-scoped content containers"
        )

        let splitResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "content-split-1",
                  "command": "pane.split",
                  "pane_id": "pane_1",
                  "direction": "horizontal"
                }
                """
            )
        )
        expect(splitResponse.applied == true, "pane.split content response setup must apply")
        expect(
            splitResponse.state != nil
                && splitResponse.paneSlotID == splitResponse.paneID
                && splitResponse.contentKind == .terminal,
            "pane.split response must include resulting state and mounted content identity"
        )
        expect(
            splitResponse.contentCapabilities?.contains(.terminalInput) == true,
            "pane.split response must expose terminal content capabilities"
        )

        let terminalHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        guard let terminalContentID = controller.shellState
            .contentStateProjection()
            .contentMounted(in: "pane_1")?
            .contentID
        else {
            fail("control-plane send-text setup must expose terminal content identity")
        }

        let paneSlotText = "echo slot"
        let paneSlotSendResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-slot-send-1",
                  "command": "terminal.send_text",
                  "pane_slot_id": "pane_1",
                  "text": "\(paneSlotText)"
                }
                """
            )
        )
        expect(
            paneSlotSendResponse.applied == true
                && paneSlotSendResponse.paneSlotID == "pane_1"
                && paneSlotSendResponse.contentID == terminalContentID
                && paneSlotSendResponse.contentKind == .terminal
                && paneSlotSendResponse.acceptedBytes == paneSlotText.lengthOfBytes(using: .utf8)
                && paneSlotSendResponse.deliveryCode == TerminalRuntimeDeliveryCode.accepted.rawValue,
            "terminal.send_text must resolve pane_slot_id to terminal content before delivery"
        )
        expect(
            terminalHandle.deliveredText == [paneSlotText],
            "pane_slot_id delivery must reach the terminal runtime"
        )

        let contentText = "echo content"
        let contentSendResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-content-send-1",
                  "command": "terminal.send_text",
                  "content_id": "\(terminalContentID)",
                  "text": "\(contentText)"
                }
                """
            )
        )
        expect(
            contentSendResponse.applied == true
                && contentSendResponse.paneSlotID == "pane_1"
                && contentSendResponse.contentID == terminalContentID
                && contentSendResponse.acceptedBytes == contentText.lengthOfBytes(using: .utf8),
            "terminal.send_text must deliver explicit terminal content_id targets"
        )
        expect(
            terminalHandle.deliveredText == [paneSlotText, contentText],
            "content_id delivery must use the terminal content runtime"
        )

        let deliveredBeforeRejections = terminalHandle.deliveredText
        let nonTerminalResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-markdown-send-1",
                  "command": "terminal.send_text",
                  "pane_slot_id": "\(markdownPaneID)",
                  "text": "ignored"
                }
                """
            )
        )
        expect(
            nonTerminalResponse.applied == false
                && nonTerminalResponse.errorCode == "unsupported_content"
                && nonTerminalResponse.paneSlotID == markdownPaneID
                && nonTerminalResponse.contentKind == .markdown
                && nonTerminalResponse.acceptedBytes == nil
                && nonTerminalResponse.deliveryCode == nil,
            "terminal.send_text must reject non-terminal PaneSlot targets"
        )
        expect(
            terminalHandle.deliveredText == deliveredBeforeRejections,
            "non-terminal terminal.send_text rejection must not deliver runtime text"
        )

        let missingPaneSlotResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-missing-slot-1",
                  "command": "terminal.send_text",
                  "pane_slot_id": "pane_missing",
                  "text": "ignored"
                }
                """
            )
        )
        expect(
            missingPaneSlotResponse.applied == false
                && missingPaneSlotResponse.errorCode == "pane_not_found"
                && missingPaneSlotResponse.paneSlotID == "pane_missing"
                && missingPaneSlotResponse.contentID == nil
                && missingPaneSlotResponse.contentKind == nil,
            "terminal.send_text must preserve missing pane_slot_id diagnostics"
        )
        expect(
            terminalHandle.deliveredText == deliveredBeforeRejections,
            "missing pane_slot_id rejection must not deliver runtime text"
        )

        let invalidContentResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "content-invalid-1",
                  "command": "terminal.send_text",
                  "pane_id": "pane_1",
                  "content_id": "content_missing",
                  "text": "echo ignored"
                }
                """
            )
        )
        expect(
            invalidContentResponse.applied == false
                && invalidContentResponse.errorCode == "content_not_found",
            "terminal.send_text must reject an explicit unknown content_id"
        )
        expect(
            invalidContentResponse.paneID == "pane_1"
                && invalidContentResponse.contentID == "content_missing",
            "unknown content_id response must preserve the requested ids"
        )
        expect(
            invalidContentResponse.paneSlotID == nil
                && invalidContentResponse.contentKind == nil
                && invalidContentResponse.contentCapabilities == nil,
            "unknown content_id response must not fall back to pane-slot content metadata"
        )

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(stateResponse),
              let json = String(data: data, encoding: .utf8)
        else {
            fail("control-plane content response must encode to JSON")
        }
        expect(
            json.contains("\"pane_slots\"")
                && json.contains("\"contents\"")
                && json.contains("\"pane_slot_id\"")
                && json.contains("\"content_capabilities\""),
            "control-plane content response JSON must use stable content-container keys"
        )
    }

    private static func verifiesControlPlanePropagatesRuntimeDeliveryFailures() {
        var deliveries: [(String, String)] = []
        let registry = TerminalRuntimeRegistry(
            runtimeService: FakeAlanTerminalRuntimeService(),
            mockDeliveryHandler: { contentID, text in
                deliveries.append((contentID, text))
                if text == "timeout" {
                    return .timeout(
                        errorMessage: "runtime command exceeded deadline",
                        runtimePhase: "bootstrapping"
                    )
                }
                return .unavailable(
                    errorMessage: "runtime is not ready",
                    runtimePhase: "bootstrapping"
                )
            }
        )
        let controller = makeController(terminalRuntimeRegistry: registry)
        guard let terminalContentID = controller.shellState
            .contentStateProjection()
            .contentMounted(in: "pane_1")?
            .contentID
        else {
            fail("runtime failure control-plane setup must expose terminal content identity")
        }

        let unavailable = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "runtime-unavailable-1",
                  "command": "terminal.send_text",
                  "pane_id": "pane_1",
                  "text": "offline"
                }
                """
            )
        )
        expect(
            unavailable.applied == false
                && unavailable.acceptedBytes == 0
                && unavailable.deliveryCode == TerminalRuntimeDeliveryCode.unavailableRuntime.rawValue
                && unavailable.runtimePhase == "bootstrapping"
                && unavailable.errorCode == "terminal_runtime_unavailable"
                && unavailable.errorMessage == "runtime is not ready",
            "control-plane send-text must preserve runtime-unavailable delivery diagnostics"
        )

        let timeout = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "runtime-timeout-1",
                  "command": "terminal.send_text",
                  "pane_id": "pane_1",
                  "text": "timeout"
                }
                """
            )
        )
        expect(
            timeout.applied == false
                && timeout.acceptedBytes == 0
                && timeout.deliveryCode == TerminalRuntimeDeliveryCode.timeout.rawValue
                && timeout.runtimePhase == "bootstrapping"
                && timeout.errorCode == "terminal_runtime_timeout",
            "control-plane send-text must preserve timeout delivery diagnostics"
        )
        expect(
            deliveries.count == 2
                && deliveries.allSatisfy { $0.0 == terminalContentID },
            "control-plane delivery failures must still target the resolved terminal content"
        )
    }

    private static func verifiesControlPlaneSendTextPreservesExplicitTerminalContentIdentity() {
        let windowID = "content_delivery_\(UUID().uuidString)"
        let baselineState = ShellStateSnapshot.bootstrapDefault(
            windowID: windowID,
            workingDirectory: "/tmp"
        )
        let baselineContentState = baselineState.contentStateProjection()
        guard let originalSlot = baselineContentState.paneSlot(paneSlotID: "pane_1"),
              let originalContent = baselineContentState.content(contentID: originalSlot.contentID),
              let pane = baselineState.pane(paneID: "pane_1")
        else {
            fail("explicit content delivery setup must expose a terminal pane")
        }

        let explicitContentID = "content_terminal_explicit_target"
        let explicitSlot = ShellPaneSlot(
            paneSlotID: originalSlot.paneSlotID,
            tabID: originalSlot.tabID,
            spaceID: originalSlot.spaceID,
            contentID: explicitContentID,
            attention: originalSlot.attention
        )
        let explicitContent = ShellContentInstance.projectingTerminalPane(
            pane,
            contentID: explicitContentID
        )
        let explicitContentState = ShellContentStateSnapshot(
            contractVersion: baselineContentState.contractVersion,
            windowID: baselineContentState.windowID,
            focusedSpaceID: baselineContentState.focusedSpaceID,
            focusedTabID: baselineContentState.focusedTabID,
            focusedPaneSlotID: baselineContentState.focusedPaneSlotID,
            spaces: baselineContentState.spaces,
            paneSlots: baselineContentState.paneSlots.map { paneSlot in
                paneSlot.paneSlotID == originalSlot.paneSlotID ? explicitSlot : paneSlot
            },
            contents: baselineContentState.contents.filter {
                $0.contentID != originalContent.contentID
            } + [explicitContent]
        )
        guard let shellState = explicitContentState.materializingShellState() else {
            fail("explicit terminal content state must materialize")
        }

        var deliveries: [(String, String)] = []
        let registry = TerminalRuntimeRegistry(
            runtimeService: FakeAlanTerminalRuntimeService(),
            mockDeliveryHandler: { contentID, text in
                deliveries.append((contentID, text))
                return .accepted(byteCount: text.lengthOfBytes(using: .utf8))
            }
        )
        let controller = makeController(shellState: shellState, terminalRuntimeRegistry: registry)

        let text = "echo explicit-content"
        let response = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "terminal-explicit-content-send-1",
                  "command": "terminal.send_text",
                  "content_id": "\(explicitContentID)",
                  "text": "\(text)"
                }
                """
            )
        )

        expect(
            response.applied == true
                && response.contentID == explicitContentID
                && response.paneSlotID == "pane_1",
            "terminal.send_text must accept explicit terminal content_id targets"
        )
        expect(
            deliveries.count == 1
                && deliveries[0].0 == explicitContentID
                && deliveries[0].1 == text,
            "terminal.send_text delivery must preserve explicit content_id instead of deriving from pane slot"
        )
    }

    private static func verifiesControlFilePollerHandlesMalformedCommandFiles() {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("MalformedControl-\(UUID().uuidString)", isDirectory: true)
        let commandsURL = root.appendingPathComponent("commands", isDirectory: true)
        let resultsURL = root.appendingPathComponent("results", isDirectory: true)
        try! FileManager.default.createDirectory(at: commandsURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let malformedURL = commandsURL.appendingPathComponent("bad.json")
        try! "{ not valid json".write(to: malformedURL, atomically: true, encoding: .utf8)
        var handledCommands = 0
        var diagnostics: [String] = []
        let poller = AlanShellControlFilePoller(
            windowID: "poller_malformed",
            fileManager: .default,
            commandsURL: commandsURL,
            resultsURL: resultsURL,
            encoder: JSONEncoder(),
            decoder: JSONDecoder(),
            commandHandler: { command in
                handledCommands += 1
                return controlPlaneTestResponse(requestID: command.requestID, applied: true)
            },
            bindingProjectionHandler: { _, _ in },
            diagnosticHandler: { diagnostics.append($0) }
        )

        poller.pollCommandsOnce()

        expect(handledCommands == 0, "malformed control command files must not reach the handler")
        expect(
            !FileManager.default.fileExists(atPath: malformedURL.path),
            "malformed control command files must be removed after diagnostics"
        )
        expect(
            diagnostics.contains { $0.contains("Ignored unreadable shell command file bad.json") },
            "malformed control command files must emit an IO diagnostic"
        )
    }

    private static func verifiesControlFilePollerReportsResultWriteDiagnostics() {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ResultWriteControl-\(UUID().uuidString)", isDirectory: true)
        let commandsURL = root.appendingPathComponent("commands", isDirectory: true)
        let resultsURL = root.appendingPathComponent("results-file")
        try! FileManager.default.createDirectory(at: commandsURL, withIntermediateDirectories: true)
        try! "not a directory".write(to: resultsURL, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: root) }

        let commandURL = commandsURL.appendingPathComponent("io.json")
        try! """
        {
          "request_id": "io-1",
          "command": "state"
        }
        """.write(to: commandURL, atomically: true, encoding: .utf8)
        var handledRequestIDs: [String] = []
        var diagnostics: [String] = []
        let poller = AlanShellControlFilePoller(
            windowID: "poller_io",
            fileManager: .default,
            commandsURL: commandsURL,
            resultsURL: resultsURL,
            encoder: JSONEncoder(),
            decoder: JSONDecoder(),
            commandHandler: { command in
                handledRequestIDs.append(command.requestID)
                return controlPlaneTestResponse(requestID: command.requestID, applied: true)
            },
            bindingProjectionHandler: { _, _ in },
            diagnosticHandler: { diagnostics.append($0) }
        )

        poller.pollCommandsOnce()

        expect(handledRequestIDs == ["io-1"], "valid control command files must still run")
        expect(
            diagnostics.contains {
                $0.contains("Failed to write shell command result io-1.json")
            },
            "result write failures must emit a stable IO diagnostic"
        )
        expect(
            !FileManager.default.fileExists(atPath: commandURL.path),
            "processed control command files must be removed after result write diagnostics"
        )
    }

    private static func verifiesContentContainerEventsCaptureLifecycleAndRejections() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("Events-\(UUID().uuidString).md")
        do {
            try "## Event notes\n".write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("content event setup must create a markdown file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let controller = makeController()
        controller.controlPlane.publish(state: controller.shellState)
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(fileURL: fileURL, title: "Event Notes")
        ) else {
            fail("content event setup must create markdown content")
        }
        guard let markdownContent = controller.shellState
            .contentStateProjection()
            .contentMounted(in: markdownPaneID)
        else {
            fail("content event setup must expose markdown content")
        }

        let splitEvents = controlEvents(controller)
        expect(
            splitEvents.contains {
                $0.type == "pane_slot.created"
                    && $0.payload["pane_slot_id"] == .string(markdownPaneID)
                    && $0.payload["content_id"] == .string(markdownContent.contentID)
                    && $0.payload["content_kind"] == .string(ShellContentKind.markdown.rawValue)
            },
            "markdown split must emit PaneSlot creation event with mounted content"
        )
        expect(
            splitEvents.contains {
                $0.type == "content.created"
                    && $0.payload["pane_slot_id"] == .string(markdownPaneID)
                    && $0.payload["content_id"] == .string(markdownContent.contentID)
                    && $0.payload["content_kind"] == .string(ShellContentKind.markdown.rawValue)
            },
            "markdown split must emit content creation event"
        )

        let rejectionResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "content-event-reject-1",
                  "command": "terminal.send_text",
                  "pane_slot_id": "\(markdownPaneID)",
                  "text": "ignored"
                }
                """
            )
        )
        expect(
            rejectionResponse.applied == false
                && rejectionResponse.errorCode == "unsupported_content",
            "content event setup must reject terminal command for markdown content"
        )
        expect(
            controlEvents(controller).contains {
                $0.type == "content.command_rejected"
                    && $0.payload["request_id"] == .string("content-event-reject-1")
                    && $0.payload["command"] == .string("terminal.send_text")
                    && $0.payload["pane_slot_id"] == .string(markdownPaneID)
                    && $0.payload["content_id"] == .string(markdownContent.contentID)
                    && $0.payload["content_kind"] == .string(ShellContentKind.markdown.rawValue)
                    && $0.payload["error_code"] == .string("unsupported_content")
            },
            "unsupported content command must emit a rejected command event"
        )

        expect(
            controller.closePane(paneID: markdownPaneID) == .closed,
            "content event setup must close markdown PaneSlot"
        )
        let closeEvents = controlEvents(controller)
        expect(
            closeEvents.contains {
                $0.type == "pane_slot.closed"
                    && $0.payload["pane_slot_id"] == .string(markdownPaneID)
                    && $0.payload["content_id"] == .string(markdownContent.contentID)
            },
            "closing markdown content must emit PaneSlot closure event"
        )
        expect(
            closeEvents.contains {
                $0.type == "content.closed"
                    && $0.payload["pane_slot_id"] == .string(markdownPaneID)
                    && $0.payload["content_id"] == .string(markdownContent.contentID)
                    && $0.payload["content_kind"] == .string(ShellContentKind.markdown.rawValue)
            },
            "closing markdown content must emit content closure event"
        )

        let replacementController = makeController()
        replacementController.controlPlane.publish(state: replacementController.shellState)
        let baselineContentState = replacementController.shellState.contentStateProjection()
        guard let originalSlot = baselineContentState.paneSlot(paneSlotID: "pane_1"),
              let originalContent = baselineContentState.content(contentID: originalSlot.contentID)
        else {
            fail("replacement event setup must expose original terminal content")
        }
        let replacementContent = ShellContentInstance(
            contentID: "content_replacement_markdown",
            kind: .markdown,
            title: "Replacement.md",
            payload: .markdown(
                ShellMarkdownContentPayload(
                    fileURL: fileURL.standardizedFileURL.absoluteString,
                    title: "Replacement.md"
                )
            ),
            rendererState: ShellContentRendererState(phase: "ready", detail: fileURL.path)
        )
        let replacementSlot = ShellPaneSlot(
            paneSlotID: originalSlot.paneSlotID,
            tabID: originalSlot.tabID,
            spaceID: originalSlot.spaceID,
            contentID: replacementContent.contentID,
            attention: originalSlot.attention
        )
        let replacementContentState = ShellContentStateSnapshot(
            contractVersion: baselineContentState.contractVersion,
            windowID: baselineContentState.windowID,
            focusedSpaceID: baselineContentState.focusedSpaceID,
            focusedTabID: baselineContentState.focusedTabID,
            focusedPaneSlotID: baselineContentState.focusedPaneSlotID,
            spaces: baselineContentState.spaces,
            paneSlots: baselineContentState.paneSlots.map { paneSlot in
                paneSlot.paneSlotID == originalSlot.paneSlotID ? replacementSlot : paneSlot
            },
            contents: baselineContentState.contents.filter {
                $0.contentID != originalContent.contentID
            } + [replacementContent]
        )
        guard let replacementState = replacementContentState.materializingShellState() else {
            fail("replacement content state must materialize shell state")
        }
        replacementController.controlPlane.publish(state: replacementState)

        expect(
            controlEvents(replacementController).contains {
                $0.type == "content.replaced"
                    && $0.payload["pane_slot_id"] == .string(originalSlot.paneSlotID)
                    && $0.payload["previous_content_id"] == .string(originalContent.contentID)
                    && $0.payload["previous_content_kind"] == .string(ShellContentKind.terminal.rawValue)
                    && $0.payload["current_content_id"] == .string(replacementContent.contentID)
                    && $0.payload["current_content_kind"] == .string(ShellContentKind.markdown.rawValue)
            },
            "same PaneSlot content replacement must emit replacement event"
        )
    }

    private static func verifiesMixedContentPaneSlotMutationsStayContentAgnostic() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("Mixed-\(UUID().uuidString).md")
        do {
            try "## Mixed notes\n".write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("mixed content setup must create a markdown file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let controller = makeController()
        let terminalHandle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let terminalContentID = controller.shellState
            .contentStateProjection()
            .contentMounted(in: "pane_1")?
            .contentID
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(fileURL: fileURL, title: nil)
        ) else {
            fail("mixed content setup must create a markdown split")
        }
        guard let settingsPaneID = controller.splitPane(
            paneID: markdownPaneID,
            placement: .down,
            contentIntent: .settings(title: nil)
        ) else {
            fail("mixed content setup must create a settings split")
        }
        guard let mixedTabID = controller.shellState.pane(paneID: "pane_1")?.tabID else {
            fail("mixed content setup must keep the source tab")
        }

        var projection = controller.shellState.contentStateProjection()
        let markdownContentID = projection.contentMounted(in: markdownPaneID)?.contentID
        let settingsContentID = projection.contentMounted(in: settingsPaneID)?.contentID
        expect(
            projection.tab(tabID: mixedTabID)?.paneTree.paneSlotIDs == [
                "pane_1",
                markdownPaneID,
                settingsPaneID,
            ],
            "mixed split setup must keep terminal, markdown, and settings PaneSlots in one tree"
        )
        expect(
            projection.contentMounted(in: markdownPaneID)?.kind == .markdown,
            "mixed split setup must mount markdown content"
        )
        expect(
            projection.contentMounted(in: settingsPaneID)?.kind == .settings,
            "mixed split setup must mount settings content"
        )
        expect(
            terminalContentID == projection.contentMounted(in: "pane_1")?.contentID,
            "mixed split setup must preserve terminal content identity"
        )
        expect(
            !controller.terminalRuntimeRegistry.registeredPaneIDs.contains(markdownPaneID),
            "markdown PaneSlot must not allocate a terminal runtime"
        )
        expect(
            !controller.terminalRuntimeRegistry.registeredPaneIDs.contains(settingsPaneID),
            "settings PaneSlot must not allocate a terminal runtime"
        )

        controller.focus(paneID: settingsPaneID)
        projection = controller.shellState.contentStateProjection()
        expect(
            projection.focusedContent?.kind == .settings,
            "PaneSlot focus must move from terminal to settings content"
        )
        expect(
            terminalHandle.teardownCount == 0,
            "focusing non-terminal content must not finalize the terminal runtime"
        )

        guard let splitNodeID = controller.shellState
            .tab(tabID: mixedTabID)?
            .paneTree
            .splitNodes
            .first?
            .nodeID
        else {
            fail("mixed split setup must expose a resizable split node")
        }
        expect(controller.resizeSplit(splitNodeID: splitNodeID, ratio: 0.7), "mixed split resize must apply")
        expect(controller.equalizeSelectedTabSplits(), "mixed split equalize must apply")
        expect(
            terminalHandle.teardownCount == 0,
            "resize and equalize must preserve terminal runtime identity"
        )

        guard let targetTabID = controller.openTerminalTab(in: controller.selectedSpaceID) else {
            fail("mixed content move setup must create a target tab")
        }
        expect(
            controller.movePane(paneID: markdownPaneID, toTab: targetTabID, direction: .horizontal),
            "cross-tab move must accept a markdown PaneSlot"
        )
        projection = controller.shellState.contentStateProjection()
        expect(
            projection.paneSlot(paneSlotID: markdownPaneID)?.tabID == targetTabID,
            "moved markdown PaneSlot must adopt the target tab membership"
        )
        expect(
            controller.shellState.paneSlots?.first { $0.paneSlotID == markdownPaneID }?.tabID == targetTabID,
            "cross-tab move must persist markdown PaneSlot target tab membership"
        )
        expect(
            projection.paneSlot(paneSlotID: markdownPaneID)?.spaceID == controller.shellState.focusedSpaceID,
            "moved markdown PaneSlot must adopt the target space membership"
        )
        expect(
            projection.contentMounted(in: markdownPaneID)?.contentID == markdownContentID,
            "cross-tab move must preserve markdown ContentInstance identity"
        )
        expect(
            projection.tab(tabID: mixedTabID)?.paneTree.paneSlotIDs.contains(markdownPaneID) == false,
            "source split tree must drop the moved markdown PaneSlot"
        )
        expect(
            projection.tab(tabID: targetTabID)?.paneTree.paneSlotIDs.contains(markdownPaneID) == true,
            "target split tree must include the moved markdown PaneSlot"
        )
        expect(
            terminalHandle.teardownCount == 0,
            "moving non-terminal content must preserve sibling terminal runtime"
        )

        expect(
            controller.closePane(paneID: markdownPaneID) == .closed,
            "closing moved markdown PaneSlot must apply"
        )
        projection = controller.shellState.contentStateProjection()
        expect(
            projection.paneSlot(paneSlotID: markdownPaneID) == nil,
            "closing markdown PaneSlot must remove the PaneSlot descriptor"
        )
        expect(
            markdownContentID.flatMap { projection.content(contentID: $0) } == nil,
            "closing markdown PaneSlot must remove the mounted ContentInstance"
        )
        expect(
            terminalHandle.teardownCount == 0,
            "closing markdown PaneSlot must not call terminal finalizer"
        )

        expect(
            controller.liftPaneToTab(paneID: settingsPaneID) == .lifted,
            "PaneSlot lift must accept settings content"
        )
        projection = controller.shellState.contentStateProjection()
        guard let liftedSettingsSlot = projection.paneSlot(paneSlotID: settingsPaneID) else {
            fail("lifted settings PaneSlot must remain in content state")
        }
        expect(
            liftedSettingsSlot.tabID != mixedTabID,
            "lifted settings PaneSlot must adopt the new tab membership"
        )
        expect(
            controller.shellState.paneSlots?.first { $0.paneSlotID == settingsPaneID }?.tabID
                == liftedSettingsSlot.tabID,
            "PaneSlot lift must persist settings target tab membership"
        )
        expect(
            projection.contentMounted(in: settingsPaneID)?.contentID == settingsContentID,
            "PaneSlot lift must preserve settings ContentInstance identity"
        )
        expect(
            controller.terminalRuntimeRegistry.registeredPaneIDs.contains(settingsPaneID) == false,
            "lifted settings PaneSlot must not allocate a terminal runtime"
        )

        expect(
            controller.closeTab(tabID: liftedSettingsSlot.tabID) == .closed,
            "closing settings tab must apply"
        )
        projection = controller.shellState.contentStateProjection()
        expect(
            settingsContentID.flatMap { projection.content(contentID: $0) } == nil,
            "closing settings tab must remove the settings ContentInstance"
        )
        expect(
            terminalHandle.teardownCount == 0,
            "closing settings tab must not finalize unrelated terminal runtime"
        )
    }

    private static func verifiesChannelScopedSupportStatePaths() {
        let fileManager = FileManager.default
        let stableManifest = ShellWorkspaceManifestStore.defaultManifestURL(
            windowID: "window_main",
            fileManager: fileManager,
            channel: .stable
        )
        let devManifest = ShellWorkspaceManifestStore.defaultManifestURL(
            windowID: "window_main",
            fileManager: fileManager,
            channel: .dev
        )
        let stableControl = alanShellControlPlaneRootURL(
            windowID: "window_main",
            channel: .stable
        )
        let devControl = alanShellControlPlaneRootURL(
            windowID: "window_main",
            channel: .dev
        )

        expect(stableManifest != devManifest, "stable and dev shell manifest paths must differ")
        expect(stableControl != devControl, "stable and dev temporary control roots must differ")
        expect(
            stableManifest.path.contains("/alan-macos/"),
            "stable shell manifest path must remain under alan-macos"
        )
        expect(
            devManifest.path.contains("/alan-macos-dev/"),
            "dev shell manifest path must be under alan-macos-dev"
        )
    }

    private static func verifiesSmokeEnvironmentPathOverrides() {
        let supportOverride = FileManager.default.temporaryDirectory
            .appendingPathComponent("alan smoke support", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let supportDirectory = alanMacApplicationSupportDirectory(
            environment: [
                "ALAN_MACOS_APPLICATION_SUPPORT_DIR": " \(supportOverride.path) "
            ]
        )
        expect(
            supportDirectory.path == supportOverride.path,
            "app support directory override must trim and expand a smoke path"
        )

        let namespace = alanShellControlNamespace(
            channel: .dev,
            environment: [
                "ALAN_SHELL_CONTROL_NAMESPACE": " smoke namespace/with spaces "
            ]
        )
        expect(
            namespace == "smoke-namespace-with-spaces",
            "shell control namespace override must be sanitized for filesystem use"
        )
    }

    private static func verifiesWorkspaceManifestStartupRestoresPinnedSnapshot() {
        let windowID = "manifest_startup_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let context = ShellWindowContext.make(windowID: windowID)
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        var manifest = contentManifest(windowID: windowID, cwd: "/live", transcriptSnapshot: nil)
        manifest.spaces[0].tabs[0].isPinned = true
        manifest.spaces[0].tabs[0].pinSnapshot = contentRestoreSnapshot(
            paneSlotID: "pane_1",
            contentID: "content_pane_1",
            cwd: "/pinned",
            transcriptSnapshot: nil
        )

        do {
            try store.save(manifest)
        } catch {
            fail("failed to write test manifest: \(error)")
        }

        let controller = ShellHostController.live(
            windowContext: context,
            workspaceManifestURL: manifestURL,
            defaultWorkingDirectory: "/fallback",
            now: Date(timeIntervalSince1970: 20)
        )

        expect(controller.selectedPane?.cwd == "/pinned", "workspace manifest startup must use pinned cwd")
        expect(
            controller.shellState.focusedSpaceID == "space_main",
            "workspace manifest startup must preserve selected space"
        )
        expect(
            controller.shellState.focusedTabID == "tab_main",
            "workspace manifest startup must preserve selected tab"
        )
        guard let restoredManifest = decodeManifest(at: manifestURL),
              let restoredTab = restoredManifest.spaces.flatMap(\.tabs).first
        else {
            fail("workspace manifest startup must retain the current manifest")
        }
        expect(
            restoredTab.pinSnapshot?.paneSlots.first?.paneSlotID == "pane_1",
            "current manifest restore must preserve PaneSlot identity"
        )
        expect(
            restoredTab.pinSnapshot?.contents.first?.payload.terminal?.cwd == "/pinned",
            "current manifest restore must preserve terminal restore payload"
        )
    }

    private static func verifiesWorkspaceManifestStartupSeedsRestoredTerminalTranscript() {
        let windowID = "manifest_seed_transcript_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let context = ShellWindowContext.make(
            windowID: windowID,
            terminalRuntimeRegistry: registry
        )
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let transcript = TerminalTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/repo/app",
            title: "npm run dev",
            dimensions: TerminalTranscriptDimensions(columns: 120, rows: 30),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 7, cursorRow: 29),
            transcriptLines: ["server ready", "listening on 3000"],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "foreground_command",
                program: "npm",
                argvPreview: ["npm", "run", "dev"],
                lastCommandExitCode: nil
            ),
            capturedAt: Date(timeIntervalSince1970: 94),
            alternateScreen: false
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellContentWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: windowID,
            selectedSpaceID: "space_main",
            selectedTabID: "tab_main",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: Date(timeIntervalSince1970: 94),
                    updatedAt: Date(timeIntervalSince1970: 94),
                    tabs: [
                        ShellContentWorkspaceTabRecord(
                            tabID: "tab_main",
                            title: "Shell",
                            kind: .terminal,
                            createdAt: Date(timeIntervalSince1970: 94),
                            lastActivatedAt: Date(timeIntervalSince1970: 94),
                            lastActivityAt: Date(timeIntervalSince1970: 94),
                            isPinned: false,
                            pinSnapshot: nil,
                            liveSnapshot: contentRestoreSnapshot(
                                paneSlotID: "pane_1",
                                contentID: "content_pane_1",
                                cwd: "/repo/app",
                                transcriptSnapshot: transcript
                            ),
                            activeTask: .foregroundCommand
                        )
                    ]
                )
            ]
        )

        do {
            try store.save(manifest)
        } catch {
            fail("test setup must write transcript manifest: \(error)")
        }

        let controller = ShellHostController.live(
            windowContext: context,
            workspaceManifestURL: manifestURL,
            defaultWorkingDirectory: "/fallback",
            now: Date(timeIntervalSince1970: 95)
        )

        expect(
            controller.shellState.contentStateProjection()
                .contentMounted(in: "pane_1")?
                .payload.terminal?
                .transcriptSnapshot?
                .transcriptLines == ["server ready", "listening on 3000"],
            "content-state projection must preserve restored transcript payload until runtime capture replaces it"
        )

        guard let pane = controller.pane(paneID: "pane_1"),
              let visibleTranscript = controller.restoredTranscriptSnapshot(for: pane),
              let bootProfile = controller.bootProfile(for: pane),
              let handle = registry.surfaceHandle(
                for: pane,
                bootProfile: bootProfile
              ) as? FakeAlanTerminalSurfaceHandle
        else {
            fail("workspace manifest restore must create a terminal runtime handle")
        }

        expect(
            visibleTranscript.transcriptLines == ["server ready", "listening on 3000"],
            "restored terminal transcript must be available to the terminal leaf as visible history"
        )
        expect(
            bootProfile.workingDirectory == "/repo/app",
            "restored terminal runtime must start a fresh shell in the restored cwd"
        )
        expect(
            handle.seededTranscriptSnapshot?.transcriptLines == ["server ready", "listening on 3000"],
            "restored terminal runtime must be seeded with manifest transcript history before input"
        )
        expect(
            handle.seededTranscriptSnapshot?.title == "npm run dev",
            "restored terminal runtime must preserve transcript title metadata"
        )
        expect(
            handle.seededTranscriptSnapshot?.transcriptLines.contains { line in
                line.localizedCaseInsensitiveContains("restored session")
            } == false,
            "restored terminal transcript must not inject a normal-mode restored-session banner"
        )
    }

    private static func verifiesRestoredTranscriptPanelPresentationMatchesTerminalLayout() {
        let snapshot = restoredTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/repo/app",
            title: "npm run dev",
            lines: (0..<20).map { "row-\($0)" }
        )
        let presentation = RestoredTerminalTranscriptPanelPresentation(snapshot: snapshot)

        expect(
            presentation.fontSize >= 13,
            "restored transcript panel must use a terminal-sized monospace font"
        )
        expect(
            presentation.leadingInset == 0,
            "restored transcript text must share the live terminal text column"
        )
        expect(
            presentation.visibleRows == RestoredTerminalTranscriptPanelPresentation.maxVisibleRows,
            "restored transcript panel must bound visible history without shrinking text width"
        )
        expect(
            presentation.height == CGFloat(presentation.visibleRows)
                * RestoredTerminalTranscriptPanelPresentation.rowHeight
                + RestoredTerminalTranscriptPanelPresentation.verticalInset * 2,
            "restored transcript panel height must derive from terminal-like row metrics"
        )
    }

    private static func verifiesRestoredTranscriptDismissalClearsStateCacheAndManifest() {
        let windowID = "manifest_clear_transcript_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let transcript = restoredTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/repo/app",
            title: "npm run dev",
            lines: ["server ready", "listening on 3000"]
        )
        let manifest = contentManifest(
            windowID: windowID,
            cwd: "/repo/app",
            transcriptSnapshot: transcript
        )
        let controller = makeController(
            windowID: windowID,
            shellState: materializeManifestWithShellCore(
                manifest: manifest,
                defaultWorkingDirectory: "/fallback",
                now: Date(timeIntervalSince1970: 102)
            ),
            terminalRuntimeRegistry: registry,
            workspaceManifestStore: store,
            workspaceManifest: manifest
        )

        guard let pane = controller.pane(paneID: "pane_1") else {
            fail("test setup must create restored pane")
        }
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        expect(
            handle.seededTranscriptSnapshot?.transcriptLines == ["server ready", "listening on 3000"],
            "test setup must seed restored transcript into the runtime"
        )

        expect(
            controller.clearRestoredTranscriptSnapshot(forTerminalContentID: "content_pane_1"),
            "clear restored transcript must report that it removed a transcript"
        )
        expect(
            controller.restoredTranscriptSnapshot(for: pane) == nil,
            "cleared restored transcript must disappear from shell state"
        )
        expect(
            handle.seededTranscriptSnapshot == nil,
            "cleared restored transcript must disappear from the runtime cache and mounted handle"
        )

        guard let savedManifest = decodeManifest(at: manifestURL),
              let tab = savedManifest.spaces.flatMap(\.tabs).first
        else {
            fail("clearing restored transcript must persist the workspace manifest")
        }
        expect(
            terminalPayload(in: tab.liveSnapshot, paneSlotID: "pane_1")?.transcriptSnapshot == nil,
            "cleared restored transcript must not be written back to future live manifest snapshots"
        )
    }

    private static func verifiesTerminalClearActionDismissesRestoredTranscriptAndForwardsClear() {
        let windowID = "manifest_clear_action_\(UUID().uuidString)"
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let transcript = restoredTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/repo/app",
            title: "npm run dev",
            lines: ["old output"]
        )
        let manifest = contentManifest(
            windowID: windowID,
            cwd: "/repo/app",
            transcriptSnapshot: transcript
        )
        let controller = makeController(
            windowID: windowID,
            shellState: materializeManifestWithShellCore(
                manifest: manifest,
                defaultWorkingDirectory: "/fallback",
                now: Date(timeIntervalSince1970: 103)
            ),
            terminalRuntimeRegistry: registry,
            workspaceManifest: manifest
        )
        guard let pane = controller.pane(paneID: "pane_1") else {
            fail("test setup must create clear-action pane")
        }
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)

        let result = controller.performShellAction(.terminalClear)

        expect(result == .executed, "terminal clear action must execute for terminal panes")
        expect(
            controller.restoredTranscriptSnapshot(for: pane) == nil,
            "terminal clear action must dismiss the restored transcript panel"
        )
        expect(
            handle.seededTranscriptSnapshot == nil,
            "terminal clear action must clear the restored transcript runtime seed"
        )
        expect(
            handle.deliveredText.last == "\u{0c}",
            "terminal clear action must forward a clear control character to the live terminal"
        )
    }

    private static func verifiesClosingLastTabLeavesSelectedSpaceEmptyAndPersistsManifest() {
        let windowID = "manifest_close_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 30)
            )
        )

        let result = controller.closeTab(tabID: "tab_main")

        expect(result == .closed, "closing the last tab in a space must succeed")
        expect(controller.shellState.spaces.count == 1, "closing the last tab must keep its space")
        expect(
            controller.shellState.spaces.first?.tabs.isEmpty == true,
            "closing the last tab must leave the selected space empty"
        )
        expect(controller.shellState.panes.isEmpty, "closing the last tab must remove its panes")
        expect(controller.selectedSpaceID == "space_main", "empty selected space must stay selected")
        expect(controller.selectedTabID == nil, "empty selected space must clear selected tab")

        guard let savedManifest = decodeManifest(at: manifestURL) else {
            fail("closing the last tab must persist workspace manifest")
        }
        expect(savedManifest.spaces.count == 1, "persisted manifest must keep empty space")
        expect(savedManifest.spaces.first?.tabs.isEmpty == true, "persisted manifest must keep space tabless")
        expect(savedManifest.selectedSpaceID == "space_main", "persisted manifest must keep selected space")
        expect(savedManifest.selectedTabID == nil, "persisted manifest must clear selected tab")
    }

    private static func verifiesExplicitSpaceDeletionRemovesManifestSpace() {
        let windowID = "manifest_delete_space_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 40)
            )
        )
        _ = controller.createTerminalSpace(title: "Delete Me", workingDirectory: "/delete-me")

        expect(controller.deleteSpace(spaceID: "space_2"), "explicit delete-space must be accepted")
        expect(controller.shellState.space(spaceID: "space_2") == nil, "deleted space must leave shell state")

        guard let savedManifest = decodeManifest(at: manifestURL) else {
            fail("delete-space must persist workspace manifest")
        }
        expect(savedManifest.spaces.map(\.spaceID) == ["space_main"], "deleted space must leave manifest")
        expect(
            savedManifest.spaces.flatMap(\.tabs).allSatisfy { $0.tabID != "tab_2" },
            "delete-space must remove deleted space tabs from manifest"
        )
    }

    private static func verifiesPinSnapshotIsExplicitAndDoesNotTrackTransientChanges() {
        let windowID = "manifest_pin_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 50)
            )
        )

        controller.updateTerminalMetadata(metadata(title: "Pinned", cwd: "/pinned"), for: "pane_1")
        _ = controller.splitPane(paneID: "pane_1", placement: .right)
        expect(controller.pinTab(tabID: "tab_main"), "pin-tab must be accepted")

        controller.updateTerminalMetadata(metadata(title: "Moved", cwd: "/moved"), for: "pane_1")
        _ = controller.splitPane(paneID: "pane_1", placement: .down)

        guard let savedManifest = decodeManifest(at: manifestURL),
              let tab = savedManifest.spaces.flatMap(\.tabs).first(where: { $0.tabID == "tab_main" })
        else {
            fail("pin-tab must persist manifest tab")
        }

        expect(
            rawManifestText(at: manifestURL)?.contains("\"panes\"") == false,
            "persisted workspace manifest must contain only current content records"
        )
        expect(tab.isPinned, "pin-tab must mark the tab as pinned")
        expect(tab.pinSnapshot?.paneTree.paneSlotIDs.count == 2, "pin snapshot must preserve split layout at pin time")
        expect(tab.liveSnapshot?.paneTree.paneSlotIDs.count == 3, "live snapshot must track later transient split changes")
        expect(
            terminalPayload(in: tab.pinSnapshot, paneSlotID: "pane_1")?.cwd == "/pinned",
            "pin snapshot must keep cwd from pin time"
        )
        expect(
            terminalPayload(in: tab.liveSnapshot, paneSlotID: "pane_1")?.cwd == "/moved",
            "live snapshot must track later cwd changes without mutating pin snapshot"
        )

        expect(controller.updatePinnedTabSnapshot(tabID: "tab_main"), "update-pin must be accepted")
        let updatedManifest = decodeManifest(at: manifestURL)
        let updatedTab = updatedManifest?.spaces.flatMap(\.tabs).first { $0.tabID == "tab_main" }
        expect(updatedTab?.pinSnapshot?.paneTree.paneSlotIDs.count == 3, "update-pin must replace pin split snapshot")
        expect(
            terminalPayload(in: updatedTab?.pinSnapshot, paneSlotID: "pane_1")?.cwd == "/moved",
            "update-pin must replace pin cwd snapshot"
        )
    }

    private static func verifiesMixedContentPinAndLiveSnapshotsPersistContentPayloads() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("Pinned-Mixed-\(UUID().uuidString).md")
        do {
            try "# Mixed pinned notes\n".write(to: fileURL, atomically: true, encoding: .utf8)
        } catch {
            fail("mixed snapshot setup must create a markdown file: \(error)")
        }
        defer { try? FileManager.default.removeItem(at: fileURL) }

        let windowID = "manifest_mixed_pin_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 70)
            )
        )
        controller.updateTerminalMetadata(metadata(title: "Pinned", cwd: "/pinned"), for: "pane_1")
        guard let markdownPaneID = controller.splitPane(
            paneID: "pane_1",
            placement: .right,
            contentIntent: .markdown(fileURL: fileURL, title: nil)
        ) else {
            fail("mixed snapshot setup must create a markdown split")
        }
        guard let settingsPaneID = controller.splitPane(
            paneID: markdownPaneID,
            placement: .down,
            contentIntent: .settings(title: nil)
        ) else {
            fail("mixed snapshot setup must create a settings split")
        }
        expect(controller.pinTab(tabID: "tab_main"), "pin-tab must persist mixed content")

        guard let savedManifest = decodeManifest(at: manifestURL),
              let tab = savedManifest.spaces.flatMap(\.tabs).first(where: { $0.tabID == "tab_main" })
        else {
            fail("pin-tab must persist mixed content manifest")
        }

        expect(
            rawManifestText(at: manifestURL)?.contains("\"panes\"") == false,
            "mixed workspace manifest must contain only current content records"
        )
        expect(
            tab.pinSnapshot?.paneTree.paneSlotIDs == ["pane_1", markdownPaneID, settingsPaneID],
            "mixed pin snapshot must preserve terminal, markdown, and settings split order"
        )
        expect(
            Set(tab.pinSnapshot?.contents.map(\.kind) ?? []) == Set([.terminal, .markdown, .settings]),
            "mixed pin snapshot must persist every mounted content kind"
        )
        expect(
            terminalPayload(in: tab.pinSnapshot, paneSlotID: "pane_1")?.cwd == "/pinned",
            "mixed pin snapshot must persist terminal restore payload"
        )
        expect(
            contentRecord(in: tab.pinSnapshot, paneSlotID: markdownPaneID)?
                .payload.markdown?.fileURL == fileURL.standardizedFileURL.absoluteString,
            "mixed pin snapshot must persist markdown file reference"
        )
        expect(
            contentRecord(in: tab.pinSnapshot, paneSlotID: settingsPaneID)?
                .payload.settings?.surfaceID == ShellContentInstance.settingsSurfaceID,
            "mixed pin snapshot must persist settings surface identity"
        )

        controller.updateTerminalMetadata(metadata(title: "Moved", cwd: "/moved"), for: "pane_1")
        controller.flushWorkspacePersistence()
        guard let updatedManifest = decodeManifest(at: manifestURL),
              let updatedTab = updatedManifest.spaces
                .flatMap(\.tabs)
                .first(where: { $0.tabID == "tab_main" })
        else {
            fail("metadata update must persist mixed live snapshot")
        }

        expect(
            terminalPayload(in: updatedTab.pinSnapshot, paneSlotID: "pane_1")?.cwd == "/pinned",
            "mixed pin snapshot must not drift after later terminal metadata updates"
        )
        expect(
            terminalPayload(in: updatedTab.liveSnapshot, paneSlotID: "pane_1")?.cwd == "/moved",
            "mixed live snapshot must track later terminal metadata updates"
        )
        expect(
            contentRecord(in: updatedTab.liveSnapshot, paneSlotID: markdownPaneID)?
                .payload.markdown?.fileURL == fileURL.standardizedFileURL.absoluteString,
            "mixed live snapshot must retain markdown restore payload"
        )
        expect(
            contentRecord(in: updatedTab.liveSnapshot, paneSlotID: settingsPaneID)?
                .payload.settings?.surfaceID == ShellContentInstance.settingsSurfaceID,
            "mixed live snapshot must retain settings restore payload"
        )
    }

    private static func verifiesOldManifestDecodesWithoutTerminalTranscriptSnapshot() {
        let json = """
        {
          "schema_version": 1,
          "content_contract_version": "0.2",
          "window_id": "window_legacy_no_transcript",
          "selected_space_id": "space_main",
          "selected_tab_id": "tab_main",
          "spaces": [{
            "space_id": "space_main",
            "title": "Main",
            "order": 0,
            "created_at": "1970-01-01T00:00:01Z",
            "updated_at": "1970-01-01T00:00:01Z",
            "tabs": [{
              "tab_id": "tab_main",
              "title": "Shell",
              "kind": "terminal",
              "created_at": "1970-01-01T00:00:01Z",
              "last_activated_at": "1970-01-01T00:00:01Z",
              "last_activity_at": "1970-01-01T00:00:01Z",
              "is_pinned": false,
              "active_task": "inactive",
              "live_snapshot": {
                "pane_tree": {
                  "node_id": "node_pane_1",
                  "kind": "pane",
                  "pane_slot_id": "pane_1"
                },
                "pane_slots": [{
                  "pane_slot_id": "pane_1",
                  "content_id": "content_pane_1"
                }],
                "contents": [{
                  "content_id": "content_pane_1",
                  "kind": "terminal",
                  "title": "Shell",
                  "payload": {
                    "terminal": {
                      "launch_target": "shell",
                      "cwd": "/legacy",
                      "title": "Shell"
                    }
                  }
                }]
              }
            }]
          }]
        }
        """
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        guard let manifest = try? decoder.decode(
            ShellContentWorkspaceManifest.self,
            from: Data(json.utf8)
        ) else {
            fail("old content manifest without transcript snapshot must decode")
        }

        let payload = manifest.spaces.first?.tabs.first?.liveSnapshot?.contents.first?.payload.terminal
        expect(payload?.cwd == "/legacy", "old manifest terminal payload must preserve cwd")
        expect(
            payload?.transcriptSnapshot == nil,
            "old manifest terminal payload must treat absent transcript snapshot as nil"
        )
    }

    private static func verifiesTerminalTranscriptSnapshotsAreBoundedThroughManifestRoundTrip() {
        let oversizedRows = (0..<(TerminalTranscriptSnapshot.defaultMaxRows + 25)).map {
            "row-\($0)"
        }
        let oversizedLine = String(
            repeating: "x",
            count: TerminalTranscriptSnapshot.defaultEncodedByteLimit + 128
        )
        let snapshot = TerminalTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/repo/app",
            title: "make test",
            dimensions: TerminalTranscriptDimensions(columns: 120, rows: 32),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 12, cursorRow: 20),
            transcriptLines: oversizedRows + [oversizedLine],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "foreground_command",
                program: "make",
                argvPreview: ["make", "test"],
                lastCommandExitCode: nil
            ),
            capturedAt: Date(timeIntervalSince1970: 90),
            alternateScreen: false
        )
        let decodedSnapshot = roundTripTerminalTranscriptSnapshot(snapshot)

        expect(decodedSnapshot?.contentID == "content_pane_1", "snapshot must preserve content identity")
        expect(decodedSnapshot?.cwd == "/repo/app", "snapshot must preserve cwd")
        expect(decodedSnapshot?.title == "make test", "snapshot must preserve terminal title")
        expect(decodedSnapshot?.dimensions?.columns == 120, "snapshot must preserve terminal columns")
        expect(decodedSnapshot?.viewport?.firstVisibleRow == 12, "snapshot must preserve viewport anchor")
        expect(
            decodedSnapshot?.transcriptLines.count ?? Int.max <= TerminalTranscriptSnapshot.defaultMaxRows,
            "manifest round trip must enforce transcript row bounds"
        )
        expect(
            decodedSnapshot?.truncation.encodedByteCount ?? Int.max
                <= TerminalTranscriptSnapshot.defaultEncodedByteLimit,
            "manifest round trip must enforce transcript byte bounds"
        )
        expect(
            decodedSnapshot?.truncation.truncatedHead == true,
            "bounded tail snapshot must record head truncation"
        )
        expect(
            decodedSnapshot?.processSummary?.processState == "foreground_command",
            "snapshot must preserve process summary"
        )
    }

    private static func verifiesPinnedRestoreOverlaysMatchingTranscriptWithoutMutatingTemplate() {
        let transcript = TerminalTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/live",
            title: "live output",
            dimensions: TerminalTranscriptDimensions(columns: 100, rows: 24),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 0, cursorRow: 1),
            transcriptLines: ["prior output"],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "inactive",
                program: "zsh",
                argvPreview: nil,
                lastCommandExitCode: 0
            ),
            capturedAt: Date(timeIntervalSince1970: 91),
            alternateScreen: false
        )
        let stalePinnedTranscript = TerminalTranscriptSnapshot(
            contentID: "content_pane_1",
            cwd: "/pinned",
            title: "pin-time output",
            dimensions: TerminalTranscriptDimensions(columns: 80, rows: 24),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 0, cursorRow: 1),
            transcriptLines: ["stale pinned output"],
            processSummary: TerminalTranscriptProcessSummary(
                processState: "inactive",
                program: "zsh",
                argvPreview: nil,
                lastCommandExitCode: 0
            ),
            capturedAt: Date(timeIntervalSince1970: 90),
            alternateScreen: false
        )
        let tab = ShellContentWorkspaceTabRecord(
            tabID: "tab_main",
            title: "Pinned",
            kind: .terminal,
            createdAt: Date(timeIntervalSince1970: 91),
            lastActivatedAt: Date(timeIntervalSince1970: 91),
            lastActivityAt: Date(timeIntervalSince1970: 91),
            isPinned: true,
            pinSnapshot: contentRestoreSnapshot(
                paneSlotID: "pane_1",
                contentID: "content_pane_1",
                cwd: "/pinned",
                transcriptSnapshot: stalePinnedTranscript
            ),
            liveSnapshot: contentRestoreSnapshot(
                paneSlotID: "pane_1",
                contentID: "content_pane_1",
                cwd: "/live",
                transcriptSnapshot: transcript
            ),
            activeTask: .inactive
        )

        let restored = tab.pinSnapshot?.overlayingTerminalTranscriptSnapshots(from: tab.liveSnapshot)
        let payload = terminalPayload(in: restored, paneSlotID: "pane_1")
        expect(payload?.cwd == "/pinned", "pinned restore must keep explicit template cwd")
        expect(
            payload?.transcriptSnapshot?.transcriptLines == ["prior output"],
            "matching live transcript must replace stale pinned terminal transcript"
        )
        expect(
            terminalPayload(in: tab.pinSnapshot, paneSlotID: "pane_1")?
                .transcriptSnapshot?.transcriptLines == ["stale pinned output"],
            "live transcript overlay must not mutate the pinned restore template"
        )

        let unmatched = ShellContentWorkspaceTabRecord(
            tabID: "tab_main",
            title: "Pinned",
            kind: .terminal,
            createdAt: Date(timeIntervalSince1970: 92),
            lastActivatedAt: Date(timeIntervalSince1970: 92),
            lastActivityAt: Date(timeIntervalSince1970: 92),
            isPinned: true,
            pinSnapshot: contentRestoreSnapshot(
                paneSlotID: "pane_1",
                contentID: "content_pane_1",
                cwd: "/pinned",
                transcriptSnapshot: nil
            ),
            liveSnapshot: contentRestoreSnapshot(
                paneSlotID: "pane_2",
                contentID: "content_pane_2",
                cwd: "/live",
                transcriptSnapshot: transcript
            ),
            activeTask: .inactive
        )
        let unmatchedPayload = terminalPayload(
            in: unmatched.pinSnapshot?.overlayingTerminalTranscriptSnapshots(from: unmatched.liveSnapshot),
            paneSlotID: "pane_1"
        )
        expect(
            unmatchedPayload?.transcriptSnapshot == nil,
            "unmatched live transcript must not mutate a pinned restore template"
        )
    }

    private static func verifiesWorkspaceManifestSyncCapturesLiveTerminalTranscript() {
        let windowID = "manifest_transcript_capture_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let service = FakeAlanTerminalRuntimeService()
        let registry = TerminalRuntimeRegistry(runtimeService: service)
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let controller = makeController(
            windowID: windowID,
            terminalRuntimeRegistry: registry,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/repo/app",
                now: Date(timeIntervalSince1970: 93)
            )
        )
        let handle = fakeSurfaceHandle(for: "pane_1", controller: controller)
        let range = AlanTerminalBufferRange(lowerBound: 0, upperBound: 2)
        handle.commandOutputTextByRange[range] = "server ready\nlistening on 3000"
        controller.updateTerminalRuntime(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: "content_pane_1",
                paneID: "pane_1",
                tabID: "tab_main",
                logicalSize: CGSize(width: 100, height: 2),
                backingSize: CGSize(width: 100, height: 2),
                displayName: nil,
                displayID: nil,
                attachedWindowTitle: "npm run dev",
                isFocused: true,
                renderer: .placeholder,
                paneMetadata: metadata(
                    title: "npm run dev",
                    cwd: "/repo/app",
                    activeTaskState: .foregroundCommand
                ),
                surfaceState: AlanTerminalSurfaceStateSnapshot(
                    readiness: .ready,
                    terminalMode: .normalBuffer,
                    scrollback: AlanTerminalScrollbackState(
                        metrics: AlanTerminalScrollbackMetrics(
                            totalRows: 2,
                            visibleRows: 2,
                            firstVisibleRow: 0,
                            mode: .normalBuffer
                        ),
                        nativeScrollbarVisible: false,
                        thumbRange: 0..<2
                    ),
                    search: nil,
                    semanticCommands: .placeholder,
                    readonly: false,
                    secureInput: false,
                    inputReady: true,
                    rendererHealth: "ready",
                    childExited: false,
                    lastUpdatedAt: Date(timeIntervalSince1970: 93)
                ),
                lastUpdatedAt: Date(timeIntervalSince1970: 93)
            )
        )
        controller.updateTerminalMetadata(
            metadata(title: "npm run dev", cwd: "/repo/app", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        controller.flushWorkspacePersistence()

        guard let savedManifest = decodeManifest(at: manifestURL),
              let transcript = terminalPayload(
                in: savedManifest.spaces.first?.tabs.first?.liveSnapshot,
                paneSlotID: "pane_1"
              )?.transcriptSnapshot
        else {
            fail("workspace manifest sync must persist live terminal transcript snapshot")
        }
        expect(
            transcript.transcriptLines == ["server ready", "listening on 3000"],
            "workspace manifest transcript snapshot must preserve captured terminal output"
        )
        expect(transcript.cwd == "/repo/app", "workspace manifest transcript snapshot must preserve cwd")
        expect(transcript.title == "npm run dev", "workspace manifest transcript snapshot must preserve title")
    }

    private static func verifiesTabOrganizationPersistsOrderPinAndSpaceOwnership() {
        let windowID = "manifest_tab_org_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID)-workspace.json")
        let store = ShellWorkspaceManifestStore(manifestURL: manifestURL)
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: store,
            workspaceManifest: defaultManifestWithShellCore(
                windowID: windowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 60)
            )
        )

        guard let secondTabID = controller.openTerminalTab(title: "Second"),
              let targetSpaceID = controller.createTerminalSpace(title: "Target")
        else {
            fail("tab organization setup must create a second tab and target space")
        }

        guard let secondPaneID = controller.shellState.panes(in: secondTabID).first?.paneID else {
            fail("second tab must have a pane")
        }
        controller.focus(paneID: secondPaneID)
        expect(controller.pinTab(tabID: secondTabID), "pinning organization action must be accepted")
        expect(
            controller.moveTabToSpace(tabID: secondTabID, targetSpaceID: targetSpaceID),
            "move-tab-to-space organization action must be accepted"
        )

        guard let savedManifest = decodeManifest(at: manifestURL),
              let targetSpace = savedManifest.spaces.first(where: { $0.spaceID == targetSpaceID }),
              let movedTab = targetSpace.tabs.first(where: { $0.tabID == secondTabID })
        else {
            fail("tab organization must persist the moved tab in the target space")
        }

        expect(movedTab.isPinned, "move-tab-to-space must preserve pin state")
        expect(movedTab.pinSnapshot != nil, "pinning through organization must persist a pin snapshot")
        expect(
            targetSpace.tabs.filter(\.isPinned).map(\.tabID).last == secondTabID,
            "moved pinned tab must be inserted in the target pinned section"
        )
        expect(
            savedManifest.spaces.first?.tabs.allSatisfy { $0.tabID != secondTabID } == true,
            "source space order must remove the moved tab"
        )
        expect(
            savedManifest.selectedSpaceID == targetSpaceID && savedManifest.selectedTabID == secondTabID,
            "moving the selected tab must persist the followed selection"
        )
    }

    private static func verifiesManifestActiveTaskProjection() {
        let foregroundURL = manifestURL("active_foreground")
        let foregroundController = makeController(
            windowID: "active_foreground_\(UUID().uuidString)",
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: foregroundURL),
            workspaceManifest: defaultManifestWithShellCore(
                windowID: "window_main",
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 60)
            )
        )
        foregroundController.updateTerminalMetadata(
            metadata(title: "make test", activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        expect(activeTask(in: foregroundURL) == .foregroundCommand, "foreground command must protect tab")
        expect(
            foregroundController.shellState.panes.first?.context?.processState == "foreground_command",
            "foreground command metadata must project into pane process state"
        )

        let idleURL = manifestURL("active_idle")
        let idleController = makeController(
            windowID: "active_idle_\(UUID().uuidString)",
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: idleURL),
            workspaceManifest: defaultManifestWithShellCore(
                windowID: "window_main",
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 61)
            )
        )
        idleController.updateTerminalMetadata(
            metadata(title: "zsh", activeTaskState: .inactive),
            for: "pane_1"
        )
        expect(activeTask(in: idleURL) == .inactive, "idle shell must be eligible for retirement")
        expect(
            idleController.shellState.panes.first?.context?.processState == "running",
            "idle shell metadata must remain running but not foreground"
        )

        let exitedURL = manifestURL("active_exited")
        let exitedController = makeController(
            windowID: "active_exited_\(UUID().uuidString)",
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: exitedURL),
            workspaceManifest: defaultManifestWithShellCore(
                windowID: "window_main",
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 62)
            )
        )
        exitedController.updateTerminalMetadata(
            metadata(title: "done", processExited: true, activeTaskState: .foregroundCommand),
            for: "pane_1"
        )
        expect(
            activeTask(in: exitedURL) == nil,
            "exited terminal must not protect a tab after lifecycle close removes it"
        )
        expect(
            exitedController.shellState.panes.isEmpty,
            "exited metadata must close the pane instead of preserving foreground state"
        )

        let activeOnlyURL = manifestURL("active_only")
        let activeOnlyController = makeController(
            windowID: "active_only_\(UUID().uuidString)",
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: activeOnlyURL),
            workspaceManifest: defaultManifestWithShellCore(
                windowID: "window_main",
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 64)
            )
        )
        activeOnlyController.updateTerminalMetadata(
            activeOnlyMetadata(activeTaskState: .inactive),
            for: "pane_1"
        )
        activeOnlyController.updateTerminalMetadata(
            activeOnlyMetadata(activeTaskState: .unknown),
            for: "pane_1"
        )
        expect(
            activeTask(in: activeOnlyURL) == .unknown,
            "active-task-only metadata changes must persist the manifest"
        )

        let alanPendingURL = manifestURL("active_alan_pending")
        let alanPendingWindowID = "active_alan_pending_\(UUID().uuidString)"
        let alanPendingController = makeController(
            windowID: alanPendingWindowID,
            shellState: stateWithAlanBinding(windowID: alanPendingWindowID, pendingRequest: true),
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: alanPendingURL),
            workspaceManifest: defaultManifestWithShellCore(
                windowID: alanPendingWindowID,
                defaultWorkingDirectory: "/tmp",
                now: Date(timeIntervalSince1970: 63)
            )
        )
        _ = alanPendingController.setAttention(.awaitingUser, for: "pane_1")
        expect(activeTask(in: alanPendingURL) == .alanPendingYield, "alan pending yield must protect tab")
    }

    private static func verifiesTerminalProfileStoreFallbackValidationAndCorruptRecovery() {
        let fileManager = FileManager.default
        let root = temporaryDirectory(named: "terminal-profile-store")
        defer { try? fileManager.removeItem(at: root) }
        let storeURL = root.appendingPathComponent("terminal-profiles.json")
        let store = TerminalProfileStore(fileManager: fileManager, storeURL: storeURL)

        let missingLoad = store.load()
        expect(
            missingLoad.document.defaultProfileID == TerminalProfileDefinition.loginShellFallback.id,
            "missing profile store must use login-shell fallback"
        )
        expect(
            missingLoad.profiles.first?.launch.kind == .loginShell,
            "missing profile store must provide login-shell launch"
        )

        let alan = TerminalProfileDefinition(
            id: "alan",
            title: "Alan",
            launch: .sudoUser(unixUser: "alan"),
            defaultWorkingDirectory: "/Users/alan",
            presentation: TerminalProfilePresentation(
                symbolName: "person.crop.circle",
                colorName: "green"
            )
        )
        let custom = TerminalProfileDefinition(
            id: "bootstrap",
            title: "Bootstrap",
            launch: .customCommand("echo token=redacted"),
            defaultWorkingDirectory: nil,
            presentation: nil
        )
        expect(
            (try? store.save(TerminalProfileDocument(defaultProfileID: "alan", profiles: [alan, custom]))) != nil,
            "valid profile document must save"
        )
        let savedLoad = store.load()
        expect(savedLoad.document.defaultProfileID == "alan", "saved default terminal profile must round-trip")
        expect(
            savedLoad.profile(id: "bootstrap")?.redactedDisplayDetail == "Custom command",
            "custom command summary must stay redacted"
        )

        let invalid = TerminalProfileDefinition(
            id: "bad",
            title: "Bad",
            launch: .sudoUser(unixUser: ""),
            defaultWorkingDirectory: nil,
            presentation: nil
        )
        let validation = TerminalProfileValidator.validate(
            TerminalProfileDocument(defaultProfileID: "bad", profiles: [invalid])
        )
        expect(
            validation.errors.contains(.missingUnixUser("bad")),
            "sudo_user profile without Unix user must be rejected"
        )

        try? "not json".write(to: storeURL, atomically: true, encoding: .utf8)
        let corruptLoad = store.load()
        expect(
            corruptLoad.recovery?.kind == .corruptStoreQuarantined,
            "corrupt profile store must be quarantined"
        )
        expect(
            fileManager.fileExists(atPath: corruptLoad.recovery?.evidenceURL.path ?? ""),
            "corrupt evidence file must be preserved"
        )
        expect(
            corruptLoad.document.profiles == [TerminalProfileDefinition.loginShellFallback],
            "corrupt profile store must fall back to login shell"
        )
    }

    private static func verifiesTerminalProfileLaunchResolutionAndEnvironmentProjection() {
        let document = TerminalProfileDocument(
            defaultProfileID: "alan",
            profiles: [
                TerminalProfileDefinition(
                    id: "alan",
                    title: "Alan",
                    launch: .sudoUser(unixUser: "alan"),
                    defaultWorkingDirectory: "/Users/alan",
                    presentation: nil
                ),
                TerminalProfileDefinition(
                    id: "root",
                    title: "Root",
                    launch: .sudoRoot,
                    defaultWorkingDirectory: nil,
                    presentation: nil
                ),
                TerminalProfileDefinition(
                    id: "custom",
                    title: "Custom",
                    launch: .customCommand("echo hello"),
                    defaultWorkingDirectory: "/tmp",
                    presentation: nil
                ),
            ]
        )
        let executableFileManager = AlwaysExecutableFileManager()

        let sudo = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: "alan",
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(sudo.terminalProfile?.id == "alan", "sudo user resolution must keep profile id")
        expect(sudo.launchPath == "/usr/bin/sudo", "sudo user profile must launch sudo directly")
        expect(sudo.arguments == ["-iu", "alan"], "sudo user profile must use structured sudo arguments")
        expect(
            sudo.surfaceCommand == "'/usr/bin/sudo' '-iu' 'alan'",
            "sudo user profile must pass command through Ghostty surface config"
        )

        let root = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: "root",
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(root.arguments == ["-i"], "sudo root profile must use structured sudo -i")

        let custom = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: "custom",
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(
            custom.launchPath == "/bin/zsh" && custom.arguments == ["-lc", "echo hello"],
            "custom command profile must use login-shell runner"
        )
        expect(custom.surfaceCommand == "echo hello", "custom command must remain the Ghostty command payload")

        let missing = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: "lab",
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(
            missing.terminalProfileState == .missing(requestedID: "lab"),
            "missing profile must be reported"
        )
        expect(missing.strategy == .loginShellEnv, "missing profile must fall back to login shell")

        let pane = ShellPane(
            paneID: "pane_profile",
            tabID: "tab_profile",
            spaceID: "space_profile",
            launchTarget: .shell,
            cwd: nil,
            process: nil,
            attention: .active,
            context: nil,
            viewport: nil,
            alanBinding: nil,
            terminalProfileID: "alan"
        )
        let state = ShellStateSnapshot.bootstrapDefault(windowID: "window_profile")
        let boot = AlanShellBootProfile.forPane(
            pane,
            shellState: state,
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(
            boot.environment["ALAN_TERMINAL_PROFILE_ID"] == "alan",
            "boot environment must expose terminal profile id"
        )
        expect(
            boot.environment["ALAN_TERMINAL_PROFILE_KIND"] == "sudo_user",
            "boot environment must expose terminal profile kind"
        )
        expect(
            boot.environment["ALAN_TERMINAL_PROFILE_STATE"] == "resolved",
            "boot environment must expose resolved profile state"
        )
        expect(
            boot.environment["EDITOR"] == nil,
            "boot environment must not impose an EDITOR"
        )
        expect(
            boot.environment["VISUAL"] == nil,
            "boot environment must not impose a VISUAL editor"
        )
        expect(
            boot.workingDirectory == "/Users/alan",
            "profile default working directory must apply when pane cwd is absent"
        )
        expect(
            boot.command.arguments.contains("/usr/bin/env"),
            "sudo user boot command must re-inject shell-control environment after sudo"
        )
        expect(
            boot.command.arguments.contains { $0.hasPrefix("ALAN_SHELL_SOCKET=") },
            "sudo user boot command must preserve the shell-control socket for the target shell"
        )
        expect(
            boot.command.arguments.contains { $0.hasPrefix("ALAN_SHELL_BINDING_FILE=") },
            "sudo user boot command must preserve the shell binding file for the target shell"
        )

        let rootPane = ShellPane(
            paneID: "pane_root_profile",
            tabID: "tab_profile",
            spaceID: "space_profile",
            launchTarget: .shell,
            cwd: nil,
            process: nil,
            attention: .active,
            context: nil,
            viewport: nil,
            alanBinding: nil,
            terminalProfileID: "root"
        )
        let rootBoot = AlanShellBootProfile.forPane(
            rootPane,
            shellState: state,
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(
            rootBoot.command.arguments.contains("/usr/bin/env"),
            "sudo root boot command must re-inject shell-control environment after sudo"
        )
        expect(
            rootBoot.command.arguments.contains { $0.hasPrefix("ALAN_SHELL_SOCKET=") },
            "sudo root boot command must preserve the shell-control socket for the target shell"
        )

        let projectedContext = ShellPaneProjectionService(fileManager: executableFileManager)
            .projectedContext(
                for: pane,
                bootProfile: boot,
                workingDirectory: nil,
                processExited: nil,
                lastCommandExitCode: nil,
                lastMetadataAt: nil,
                existing: nil
            )
        expect(
            projectedContext.terminalProfileState == "resolved",
            "pane context must expose resolved terminal profile state"
        )
        expect(
            projectedContext.terminalProfileID == "alan",
            "pane context must expose resolved terminal profile id"
        )
        expect(
            projectedContext.terminalProfileKind == "sudo_user",
            "pane context must expose terminal profile launch kind"
        )

        let missingPane = ShellPane(
            paneID: "pane_missing_profile",
            tabID: "tab_profile",
            spaceID: "space_profile",
            launchTarget: .shell,
            cwd: nil,
            process: nil,
            attention: .active,
            context: nil,
            viewport: nil,
            alanBinding: nil,
            terminalProfileID: "lab"
        )
        let missingBoot = AlanShellBootProfile.forPane(
            missingPane,
            shellState: state,
            terminalProfiles: document,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        let missingContext = ShellPaneProjectionService(fileManager: executableFileManager)
            .projectedContext(
                for: missingPane,
                bootProfile: missingBoot,
                workingDirectory: nil,
                processExited: nil,
                lastCommandExitCode: nil,
                lastMetadataAt: nil,
                existing: nil
            )
        expect(
            missingContext.terminalProfileState == "missing",
            "pane context must expose missing terminal profile state"
        )
        expect(
            missingContext.terminalProfileRequestedID == "lab",
            "pane context must preserve missing terminal profile id"
        )
        expect(
            missingContext.terminalProfileID == nil,
            "missing profile fallback must not claim a resolved profile id"
        )

        let staleResolvedContext = ShellContextSnapshot(
            workingDirectoryName: "alan",
            repositoryRoot: nil,
            gitBranch: nil,
            controlPath: nil,
            alanBindingFile: nil,
            launchStrategy: "terminal_profile_sudo_user",
            terminalProfileState: "resolved",
            terminalProfileRequestedID: "alan",
            terminalProfileID: "alan",
            terminalProfileKind: "sudo_user",
            terminalProfileTitle: "Alan",
            shellIntegrationSource: "ghostty_shell_integration",
            processState: "running",
            lastMetadataAt: nil,
            lastCommandExitCode: nil
        )
        let recreatedMissingContext = ShellPaneProjectionService(fileManager: executableFileManager)
            .projectedContext(
                for: missingPane,
                bootProfile: missingBoot,
                workingDirectory: nil,
                processExited: nil,
                lastCommandExitCode: nil,
                lastMetadataAt: nil,
                existing: staleResolvedContext
            )
        expect(
            recreatedMissingContext.terminalProfileState == "missing",
            "pane context recreation must replace stale terminal profile state"
        )
        expect(
            recreatedMissingContext.terminalProfileRequestedID == "lab",
            "pane context recreation must replace stale terminal profile requested id"
        )
        expect(
            recreatedMissingContext.terminalProfileID == nil,
            "pane context recreation must clear stale resolved terminal profile id"
        )
        expect(
            recreatedMissingContext.terminalProfileKind == nil,
            "pane context recreation must clear stale resolved terminal profile kind"
        )
        expect(
            recreatedMissingContext.terminalProfileTitle == nil,
            "pane context recreation must clear stale resolved terminal profile title"
        )

        let loginShellDefaultDocument = TerminalProfileDocument(
            defaultProfileID: TerminalProfileDefinition.loginShellFallback.id,
            profiles: [
                TerminalProfileDefinition.loginShellFallback,
                TerminalProfileDefinition(
                    id: "alan",
                    title: "Alan",
                    launch: .sudoUser(unixUser: "alan"),
                    defaultWorkingDirectory: "/Users/alan",
                    presentation: nil
                ),
            ]
        )
        let reboundState = ShellStateSnapshot.bootstrapDefault(
            windowID: "window_existing_pane",
            workingDirectory: "/Users/morris"
        ).settingTerminalProfile("alan", forSpaceID: "space_main")
        guard let reboundPane = reboundState?.pane(paneID: "pane_1") else {
            fail("rebound default state must keep the existing pane")
        }
        let reboundBoot = AlanShellBootProfile.forPane(
            reboundPane,
            shellState: reboundState ?? state,
            terminalProfiles: loginShellDefaultDocument,
            fileManager: executableFileManager,
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(
            reboundBoot.environment["ALAN_TERMINAL_PROFILE_REQUESTED_ID"] == nil,
            "existing panes without a captured profile must not read mutable Space bindings"
        )
        expect(
            reboundBoot.command.terminalProfile?.id != "alan",
            "rebinding a Space must not relaunch an existing login-shell pane through the new profile"
        )
    }

    private static func verifiesTerminalProfileRebindingPreservesSpaceIconMetadata() {
        let initialState = stateWithContext(
            windowID: "window_icon_rebind",
            context: context(
                processState: "running",
                rendererHealth: "healthy",
                surfaceReadiness: "ready",
                lastCommandExitCode: nil
            ),
            spacePresentationIconSystemName: "rectangle.stack.fill"
        )

        let reboundState = initialState.settingTerminalProfile("alan", forSpaceID: "space_1")
        expect(
            reboundState?.space(spaceID: "space_1")?.terminalProfileID == "alan",
            "rebinding a Space terminal profile must update the Space profile reference"
        )
        expect(
            reboundState?.space(spaceID: "space_1")?.presentationIconSystemName
                == "rectangle.stack.fill",
            "rebinding a Space terminal profile must preserve explicit Space icon metadata"
        )
    }

    private static func verifiesTerminalProfileReferencesPersistThroughManifestRoundTrip() {
        let now = Date(timeIntervalSince1970: 2_001)
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellContentWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_profiles",
            selectedSpaceID: "space_main",
            selectedTabID: "tab_main",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Alan",
                    order: 0,
                    createdAt: now,
                    updatedAt: now,
                    tabs: [
                        ShellContentWorkspaceTabRecord(
                            tabID: "tab_main",
                            title: "Shell",
                            kind: .terminal,
                            createdAt: now,
                            lastActivatedAt: now,
                            lastActivityAt: now,
                            isPinned: false,
                            pinSnapshot: nil,
                            liveSnapshot: ShellContentTabRestoreSnapshot(
                                paneTree: ShellPaneSlotTreeNode(
                                    nodeID: "node_pane_1",
                                    kind: .pane,
                                    direction: nil,
                                    paneSlotID: "pane_1",
                                    children: nil
                                ),
                                paneSlots: [
                                    ShellPaneSlotRestoreRecord(
                                        paneSlotID: "pane_1",
                                        contentID: "content_pane_1"
                                    )
                                ],
                                contents: [
                                    ShellContentRestoreRecord(
                                        contentID: "content_pane_1",
                                        kind: .terminal,
                                        title: "Alan shell",
                                        payload: .terminal(
                                            ShellTerminalContentPayload(
                                                launchTarget: .shell,
                                                cwd: nil,
                                                title: "Alan shell",
                                                terminalProfileID: "alan"
                                            )
                                        )
                                    )
                                ]
                            ),
                            activeTask: .inactive
                        )
                    ],
                    terminalProfileID: "alan"
                )
            ]
        )

        let data = try? JSONEncoder().encode(manifest)
        guard let data,
              let decoded = try? JSONDecoder().decode(ShellContentWorkspaceManifest.self, from: data)
        else {
            fail("profile manifest must encode and decode")
        }
        expect(
            decoded.spaces.first?.terminalProfileID == "alan",
            "space terminal_profile_id must round-trip"
        )
        let payload = terminalPayload(
            in: decoded.spaces.first?.tabs.first?.liveSnapshot,
            contentID: "content_pane_1"
        )
        expect(
            payload?.terminalProfileID == "alan",
            "terminal content terminal_profile_id must round-trip"
        )

        let json = String(decoding: data, as: UTF8.self)
        expect(!json.contains("sudo -iu"), "workspace manifest must not embed terminal profile command definitions")
        expect(!json.contains("unix_user"), "workspace manifest must not embed terminal profile Unix-user definitions")

        let state = materializeManifestWithShellCore(
            manifest: decoded,
            defaultWorkingDirectory: "/Users/morris",
            now: now
        )
        expect(
            state.space(spaceID: "space_main")?.terminalProfileID == "alan",
            "materialized space must preserve missing local profile reference"
        )
        expect(
            state.pane(paneID: "pane_1")?.terminalProfileID == "alan",
            "materialized pane must preserve terminal profile reference"
        )
        expect(
            state.pane(paneID: "pane_1")?.cwd == nil,
            "materialized profile pane must preserve nil cwd so the profile default working directory can win"
        )
        expect(
            state.contentStateProjection().content(contentID: "content_pane_1")?.payload.terminal?.terminalProfileID == "alan",
            "content projection must preserve terminal profile reference"
        )
        expect(
            state.contentStateProjection().content(contentID: "content_pane_1")?.payload.terminal?.cwd == nil,
            "content projection must preserve nil cwd for profile-bound panes"
        )

        let legacyJSON = """
        {"schema_version":1,"content_contract_version":"\(ShellContentStateSnapshot.currentContractVersion)","window_id":"legacy","selected_space_id":null,"selected_tab_id":null,"spaces":[]}
        """
        expect(
            (try? JSONDecoder().decode(ShellContentWorkspaceManifest.self, from: Data(legacyJSON.utf8))) != nil,
            "old manifest without terminal_profile_id must decode"
        )
    }

    private static func verifiesTerminalProfileInheritanceForSpacesTabsAndSplits() {
        let now = Date(timeIntervalSince1970: 2_100)
        let terminalProfiles = TerminalProfileDocument(
            defaultProfileID: "alan",
            profiles: [
                TerminalProfileDefinition(
                    id: "alan",
                    title: "Alan",
                    launch: .sudoUser(unixUser: "alan"),
                    defaultWorkingDirectory: "/Users/alan",
                    presentation: nil
                ),
                TerminalProfileDefinition(
                    id: "root",
                    title: "Root",
                    launch: .sudoRoot,
                    defaultWorkingDirectory: "/var/root",
                    presentation: nil
                ),
            ]
        )
        let executableFileManager = AlwaysExecutableFileManager()
        let base = ShellStateSnapshot.bootstrapDefault(
            windowID: "window_inheritance",
            workingDirectory: "/Users/morris"
        )
        let createdSpace = base.creatingTerminalSpace(
            title: "Alan",
            workingDirectory: nil,
            terminalProfileID: "alan",
            now: now
        ).state
        guard let alanSpaceID = createdSpace.focusedSpaceID,
              let alanPaneID = createdSpace.focusedPaneID
        else {
            fail("space creation must focus new terminal content")
        }
        expect(
            createdSpace.space(spaceID: alanSpaceID)?.terminalProfileID == "alan",
            "new space must bind selected profile"
        )
        expect(
            createdSpace.pane(paneID: alanPaneID)?.terminalProfileID == "alan",
            "first terminal in profile-bound space must use profile"
        )
        expect(
            createdSpace.pane(paneID: alanPaneID)?.cwd == nil,
            "profile-bound space without explicit cwd must let profile default working directory apply"
        )
        let firstPaneBoot = createdSpace.pane(paneID: alanPaneID).map {
            AlanShellBootProfile.forPane(
                $0,
                shellState: createdSpace,
                terminalProfiles: terminalProfiles,
                fileManager: executableFileManager,
                environment: ["SHELL": "/bin/zsh"]
            )
        }
        expect(
            firstPaneBoot?.workingDirectory == "/Users/alan",
            "profile-bound space boot must use profile default working directory"
        )

        let inheritedTab = try? createdSpace.openingTerminalTab(
            in: alanSpaceID,
            title: nil,
            workingDirectory: nil,
            now: now
        ).state
        let inheritedPane = inheritedTab?.focusedPaneID.flatMap { inheritedTab?.pane(paneID: $0) }
        expect(
            inheritedPane?.terminalProfileID == "alan",
            "new tab must inherit space profile"
        )
        expect(
            inheritedPane?.cwd == nil,
            "profile-inherited tab without explicit cwd must let profile default working directory apply"
        )
        let inheritedBoot = inheritedPane.map {
            AlanShellBootProfile.forPane(
                $0,
                shellState: inheritedTab ?? createdSpace,
                terminalProfiles: terminalProfiles,
                fileManager: executableFileManager,
                environment: ["SHELL": "/bin/zsh"]
            )
        }
        expect(
            inheritedBoot?.workingDirectory == "/Users/alan",
            "profile-inherited tab boot must use profile default working directory"
        )

        let overrideTab = try? createdSpace.openingTerminalTab(
            in: alanSpaceID,
            title: nil,
            workingDirectory: nil,
            terminalProfileID: "root",
            now: now
        ).state
        expect(
            overrideTab?.pane(paneID: overrideTab?.focusedPaneID ?? "")?.terminalProfileID == "root",
            "explicit tab profile must override space profile"
        )
        expect(
            overrideTab?.focusedPaneID.flatMap { overrideTab?.pane(paneID: $0) }?.cwd == nil,
            "explicit profile tab without explicit cwd must let profile default working directory apply"
        )

        let explicitCwdTab = try? createdSpace.openingTerminalTab(
            in: alanSpaceID,
            title: nil,
            workingDirectory: "/explicit/project",
            terminalProfileID: "root",
            now: now
        ).state
        expect(
            explicitCwdTab?.focusedPaneID.flatMap { explicitCwdTab?.pane(paneID: $0) }?.cwd == "/explicit/project",
            "explicit cwd must still override profile default working directory"
        )

        let split = try? createdSpace.splittingPane(
            alanPaneID,
            placement: .right,
            now: now
        ).state
        expect(
            split?.pane(paneID: split?.focusedPaneID ?? "")?.terminalProfileID == "alan",
            "split must inherit focused pane profile"
        )
        expect(
            split?.focusedPaneID.flatMap { split?.pane(paneID: $0) }?.cwd == nil,
            "profile-inherited split without explicit cwd must let profile default working directory apply"
        )

        let rebound = createdSpace.settingTerminalProfile("univer", forSpaceID: alanSpaceID)
        expect(
            rebound?.pane(paneID: alanPaneID)?.terminalProfileID == "alan",
            "space binding changes must not rewrite existing panes"
        )
        let reboundTab = try? rebound?.openingTerminalTab(
            in: alanSpaceID,
            title: nil,
            workingDirectory: nil,
            now: now
        ).state
        expect(
            reboundTab?.pane(paneID: reboundTab?.focusedPaneID ?? "")?.terminalProfileID == "univer",
            "new tabs after binding change must use new profile"
        )
    }

    private static func verifiesLegacyDefaultTerminalProfileDoesNotCaptureUnboundStartup() {
        let previousSupportRoot = ProcessInfo.processInfo.environment[
            "ALAN_MACOS_APPLICATION_SUPPORT_DIR"
        ]
        let previousInstallChannel = ProcessInfo.processInfo.environment["ALAN_INSTALL_CHANNEL"]
        let supportRoot = temporaryDirectory(named: "alan-legacy-default-profile")
        setenv("ALAN_MACOS_APPLICATION_SUPPORT_DIR", supportRoot.path, 1)
        setenv("ALAN_INSTALL_CHANNEL", "stable", 1)
        defer {
            restoreEnvironmentValue(previousSupportRoot, forKey: "ALAN_MACOS_APPLICATION_SUPPORT_DIR")
            restoreEnvironmentValue(previousInstallChannel, forKey: "ALAN_INSTALL_CHANNEL")
            try? FileManager.default.removeItem(at: supportRoot)
        }

        let terminalProfiles = TerminalProfileDocument(
            defaultProfileID: "alan",
            profiles: [
                TerminalProfileDefinition.loginShellFallback,
                TerminalProfileDefinition(
                    id: "alan",
                    title: "Alan",
                    launch: .sudoUser(unixUser: "alan"),
                    defaultWorkingDirectory: "/Users/alan",
                    presentation: nil
                ),
            ]
        )
        let store = TerminalProfileStore.defaultStore(
            environment: [
                "ALAN_MACOS_APPLICATION_SUPPORT_DIR": supportRoot.path,
                "ALAN_INSTALL_CHANNEL": "stable",
            ]
        )
        let inheritedCwdLocalSplitResult = AlanShellLocalCommandExecutor.execute(
            command: decodeControlCommand(
                """
                {
                  "request_id": "local-split-inherit-source-cwd",
                  "command": "pane.split",
                  "pane_id": "pane_1",
                  "direction": "horizontal"
                }
                """
            ),
            state: .bootstrapDefault(
                windowID: "local_split_inherit_source_cwd",
                workingDirectory: "/Users/morris/project"
            )
        )
        let inheritedCwdPane = inheritedCwdLocalSplitResult?.updatedState?.pane(
            paneID: inheritedCwdLocalSplitResult?.response.paneID ?? ""
        )
        expect(
            inheritedCwdLocalSplitResult?.response.applied == true,
            "local pane.split without a captured profile must apply"
        )
        expect(
            inheritedCwdPane?.terminalProfileID == nil,
            "local pane.split without a captured profile must leave profile unset"
        )
        expect(
            inheritedCwdPane?.cwd == "/Users/morris/project",
            "local pane.split without a captured profile must inherit source pane cwd"
        )
        do {
            try store.save(terminalProfiles)
        } catch {
            fail("legacy default terminal profile test must save profile store: \(error)")
        }
        expect(
            store.load().document.defaultProfileID == "alan",
            "legacy default profile id must remain preserved in the local profile store"
        )
        expect(
            terminalProfileIDForGlobalDefaultPaneCapture(environment: [
                "ALAN_MACOS_APPLICATION_SUPPORT_DIR": supportRoot.path,
                "ALAN_INSTALL_CHANNEL": "stable",
            ]) == nil,
            "legacy default profile id must not be captured for unbound terminal startup"
        )

        let controller = makeController(
            windowID: "legacy_default_profile",
            shellState: .bootstrapDefault(
                windowID: "legacy_default_profile",
                workingDirectory: "/Users/morris/project"
            )
        )
        guard let tabID = controller.openTerminalTab(),
              let paneID = controller.shellState.tab(tabID: tabID)?.paneTree.paneIDs.first,
              let pane = controller.shellState.pane(paneID: paneID)
        else {
            fail("legacy default terminal profile test must open a terminal tab")
        }

        expect(
            pane.terminalProfileID == nil,
            "new tabs in unbound Spaces must not capture a legacy global default profile"
        )
        expect(
            pane.cwd == "/Users/morris/project",
            "new unbound tabs must keep the inherited working directory for login-shell startup"
        )
        let boot = AlanShellBootProfile.forPane(
            pane,
            shellState: controller.shellState,
            terminalProfiles: terminalProfiles,
            fileManager: AlwaysExecutableFileManager(),
            environment: ["SHELL": "/bin/zsh"]
        )
        expect(
            boot.command.terminalProfile?.id == TerminalProfileDefinition.loginShellFallback.id,
            "new unbound tabs must resolve the built-in Login shell fallback"
        )
        expect(
            boot.workingDirectory == "/Users/morris/project",
            "new unbound tabs must keep the login-shell working directory"
        )

        let spaceController = makeController(
            windowID: "legacy_default_profile_space",
            shellState: .bootstrapDefault(
                windowID: "legacy_default_profile_space",
                workingDirectory: "/Users/morris/project"
            )
        )
        guard let createdSpaceID = spaceController.createTerminalSpace(
            title: "Alan",
            workingDirectory: "/Users/morris/project"
        ),
              let createdPaneID = spaceController.shellState.space(spaceID: createdSpaceID)?
                .tabs.first?.paneTree.paneIDs.first,
              let createdPane = spaceController.shellState.pane(paneID: createdPaneID)
        else {
            fail("legacy default terminal profile test must create a terminal Space")
        }
        expect(
            spaceController.shellState.space(spaceID: createdSpaceID)?.terminalProfileID == nil,
            "new unbound Spaces must not bind a legacy global default profile"
        )
        expect(
            createdPane.terminalProfileID == nil,
            "first pane in an unbound Space must use login shell"
        )
        expect(
            createdPane.cwd == "/Users/morris/project",
            "new unbound Spaces must keep the requested working directory for login shell"
        )

        let splitController = makeController(
            windowID: "legacy_default_profile_split",
            shellState: .bootstrapDefault(
                windowID: "legacy_default_profile_split",
                workingDirectory: "/Users/morris/project"
            )
        )
        guard let focusedPaneID = splitController.shellState.focusedPaneID,
              let splitPaneID = splitController.splitPane(
                paneID: focusedPaneID,
                placement: .right
              ),
              let splitPane = splitController.shellState.pane(paneID: splitPaneID)
        else {
            fail("legacy default terminal profile test must split a terminal pane")
        }
        expect(
            splitPane.terminalProfileID == nil,
            "splits from unprofiled panes must not capture a legacy global default profile"
        )
        expect(
            splitPane.cwd == "/Users/morris/project",
            "splits from unprofiled panes must keep login-shell working directory behavior"
        )

        let localResult = AlanShellLocalCommandExecutor.execute(
            command: decodeControlCommand(
                """
                {
                  "request_id": "local-tab-global-default-profile",
                  "command": "tab.open",
                  "cwd": "/Users/morris/project"
                }
                """
            ),
            state: .bootstrapDefault(
                windowID: "local_global_default_profile",
                workingDirectory: "/Users/morris/project"
            )
        )
        let localPane = localResult?.updatedState?.pane(paneID: localResult?.response.paneID ?? "")
        expect(localResult?.response.applied == true, "local tab.open must apply")
        expect(
            localPane?.terminalProfileID == nil,
            "local tab.open must not capture a legacy global default profile"
        )
        expect(
            localPane?.cwd == "/Users/morris/project",
            "local tab.open must keep login-shell working directory behavior"
        )

        let localSpaceResult = AlanShellLocalCommandExecutor.execute(
            command: decodeControlCommand(
                """
                {
                  "request_id": "local-space-global-default-profile",
                  "command": "space.create",
                  "cwd": "/Users/morris/project"
                }
                """
            ),
            state: .bootstrapDefault(
                windowID: "local_space_global_default_profile",
                workingDirectory: "/Users/morris/project"
            )
        )
        let localSpacePane = localSpaceResult?.updatedState?.pane(
            paneID: localSpaceResult?.response.paneID ?? ""
        )
        expect(localSpaceResult?.response.applied == true, "local space.create must apply")
        expect(
            localSpaceResult?.response.spaceID.flatMap {
                localSpaceResult?.updatedState?.space(spaceID: $0)
            }?.terminalProfileID == nil,
            "local space.create must not bind a legacy global default profile"
        )
        expect(
            localSpacePane?.terminalProfileID == nil,
            "local space.create must use login shell for the first pane"
        )
        expect(
            localSpacePane?.cwd == "/Users/morris/project",
            "local space.create must keep login-shell working directory behavior"
        )

        let localSplitResult = AlanShellLocalCommandExecutor.execute(
            command: decodeControlCommand(
                """
                {
                  "request_id": "local-split-global-default-profile",
                  "command": "pane.split",
                  "pane_id": "pane_1",
                  "direction": "horizontal"
                }
                """
            ),
            state: .bootstrapDefault(
                windowID: "local_split_global_default_profile",
                workingDirectory: "/Users/morris/project"
            )
        )
        let localSplitPane = localSplitResult?.updatedState?.pane(
            paneID: localSplitResult?.response.paneID ?? ""
        )
        expect(localSplitResult?.response.applied == true, "local pane.split must apply")
        expect(
            localSplitPane?.terminalProfileID == nil,
            "local pane.split must not capture a legacy global default profile"
        )
        expect(
            localSplitPane?.cwd == "/Users/morris/project",
            "local pane.split must keep login-shell working directory behavior "
                + "(pane \(String(describing: localSplitResult?.response.paneID)), "
                + "cwd \(String(describing: localSplitPane?.cwd)), "
                + "profile \(String(describing: localSplitPane?.terminalProfileID)))"
        )
    }

    private static func verifiesTerminalProfileControlPlaneOverrides() {
        let controller = makeController()
        let createSpaceResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "space-profile",
                  "command": "space.create",
                  "title": "Root",
                  "terminal_profile_id": "root"
                }
                """
            )
        )
        expect(createSpaceResponse.applied == true, "space.create with terminal_profile_id must apply")
        guard let createdSpaceID = createSpaceResponse.spaceID,
              let createdPaneID = createSpaceResponse.paneID
        else {
            fail("space.create must return created ids")
        }
        expect(
            controller.shellState.space(spaceID: createdSpaceID)?.terminalProfileID == "root",
            "space.create must bind terminal profile to the new Space"
        )
        expect(
            controller.shellState.pane(paneID: createdPaneID)?.terminalProfileID == "root",
            "space.create must apply terminal profile to first terminal"
        )

        let bindResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "space-bind-profile",
                  "command": "space.set_terminal_profile",
                  "space_id": "\(createdSpaceID)",
                  "terminal_profile_id": "univer"
                }
                """
            )
        )
        expect(bindResponse.applied == true, "space.set_terminal_profile must apply")
        expect(
            controller.shellState.space(spaceID: createdSpaceID)?.terminalProfileID == "univer",
            "space.set_terminal_profile must update the Space binding"
        )
        expect(
            controller.shellState.pane(paneID: createdPaneID)?.terminalProfileID == "root",
            "space.set_terminal_profile must not retroactively rewrite existing terminals"
        )

        let clearResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "space-clear-profile",
                  "command": "space.set_terminal_profile",
                  "space_id": "\(createdSpaceID)",
                  "terminal_profile_id": null
                }
                """
            )
        )
        expect(clearResponse.applied == true, "space.set_terminal_profile null must apply")
        expect(
            controller.shellState.space(spaceID: createdSpaceID)?.terminalProfileID == nil,
            "space.set_terminal_profile null must clear the Space binding to Login shell"
        )
        expect(
            controller.shellState.pane(paneID: createdPaneID)?.terminalProfileID == "root",
            "clearing a Space profile must not retroactively rewrite existing terminals"
        )

        let tabResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "tab-profile",
                  "command": "tab.open",
                  "space_id": "\(createdSpaceID)",
                  "terminal_profile_id": "alan"
                }
                """
            )
        )
        expect(tabResponse.applied == true, "tab.open terminal_profile_id override must apply")
        expect(
            controller.shellState.pane(paneID: tabResponse.paneID ?? "")?.terminalProfileID == "alan",
            "tab.open explicit terminal_profile_id must override Space profile"
        )

        let splitResponse = controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "split-profile",
                  "command": "pane.split",
                  "pane_id": "\(tabResponse.paneID ?? createdPaneID)",
                  "direction": "horizontal",
                  "terminal_profile_id": "lab"
                }
                """
            )
        )
        expect(splitResponse.applied == true, "pane.split terminal_profile_id override must apply")
        expect(
            controller.shellState.pane(paneID: splitResponse.paneID ?? "")?.terminalProfileID == "lab",
            "pane.split explicit terminal_profile_id must override inherited profile"
        )
    }

    private static func verifiesTerminalProfileSettingsRowsStayLocal() {
        let terminalProfiles = TerminalProfileSettingsSummary(
            profiles: [
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
        let terminalAccounts = ManagedTerminalAccountSettingsSummary(
            plans: [
                ManagedTerminalAccountPlan(
                    request: ManagedTerminalAccountRequest(accountName: "alan"),
                    status: .alreadyReady,
                    steps: []
                )
            ]
        )
        let snapshot = ShellSettingsSurfaceSnapshot.make(
            local: .current(),
            terminalProfiles: terminalProfiles,
            managedTerminalAccounts: terminalAccounts
        )
        let sectionTitles = snapshot.sections.map(\.title)
        expect(sectionTitles.contains("Terminal Profiles"), "Settings must include Terminal Profiles as local startup configuration")
        expect(sectionTitles.contains("Managed Users"), "Settings must include Managed User provisioning")
        let visibleText = snapshot.visibleText.joined(separator: " ")
        expect(!visibleText.contains("echo hidden-secret"), "normal Settings rows must redact full custom commands")
        expect(!visibleText.lowercased().contains("autologin"), "Settings copy must avoid GUI autologin wording")
        expect(visibleText.contains("terminal entry"), "Settings copy must describe terminal account entry")

        let invalidDraft = TerminalProfileEditorDraft(
            id: "alan",
            title: "Alan",
            launchKind: .sudoUser
        )
        expect(
            TerminalProfileEditor.makeDefinition(from: invalidDraft).errors.contains(.missingUnixUser("alan")),
            "Terminal Profile editor must validate structured sudo_user fields"
        )

        let customDraft = TerminalProfileEditorDraft(
            id: "bootstrap",
            title: "Bootstrap",
            launchKind: .customCommand,
            customCommand: "echo hidden-secret"
        )
        let edited = TerminalProfileEditor.upserting(
            draft: customDraft,
            into: TerminalProfileDocument.fallback
        )
        expect(edited.isValid, "Terminal Profile editor must upsert valid custom-command drafts")
        expect(
            edited.document?.profile(id: "bootstrap")?.redactedDisplayDetail.contains("hidden-secret") == false,
            "Terminal Profile editor models must keep custom commands redacted for normal display"
        )

        let managedProfile = TerminalProfileDefinition(
            id: "alan",
            title: "Alan",
            launch: .sudoUser(unixUser: "alan"),
            defaultWorkingDirectory: "/Users/alan",
            presentation: nil,
            managedTerminalAccountID: "alan"
        )
        let managedEdit = TerminalProfileEditor.upserting(
            draft: TerminalProfileEditorDraft(
                id: "alan",
                title: "Alan Root",
                launchKind: .sudoRoot,
                defaultWorkingDirectory: "/var/root",
                managedTerminalAccountID: "alan"
            ),
            into: TerminalProfileDocument(defaultProfileID: "alan", profiles: [managedProfile])
        )
        expect(
            !managedEdit.isValid,
            "Managed Terminal Profiles must be read-only in the Terminal Profile editor"
        )
    }

    private static func verifiesManagedTerminalAccountHelperPlanAndRollbackSafety() {
        let request = ManagedTerminalAccountRequest(
            accountName: "alan",
            fullName: "Alan Terminal"
        )
        let missingDiagnosis = AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .missing,
            readinessState: .accountMissing,
            accountExists: false,
            isAdmin: false,
            homeDirectoryExists: false,
            homeDirectoryMatches: false,
            shellMatches: false,
            hiddenFromLoginWindow: false,
            terminalProfileID: nil,
            ptySmokeVerified: false,
            diagnostic: nil
        )
        let plan = ManagedTerminalAccountPlanner.plan(
            request: request,
            diagnosis: missingDiagnosis
        )
        let helperKinds = plan.steps.compactMap { step -> AlanManagedUserHelperPlanStepKind? in
            guard case .helperStep(let kind) = step.kind else { return nil }
            return kind
        }
        expect(
            helperKinds == [
                .createStandardAccount,
                .hideAccount,
                .writeOwnershipMarker,
                .verifyAccount,
                .verifyManagedUserPTY,
            ],
            "Managed User creation must use only current helper-owned account and PTY steps"
        )
        expect(
            plan.steps.map(\.kind).contains(.createOrUpdateTerminalProfile),
            "Managed User creation must hand off to a canonical managed_user profile"
        )

        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("managed-user-current-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = TerminalProfileStore(
            fileManager: .default,
            storeURL: root.appendingPathComponent("terminal-profiles.json")
        )
        let manualProfile = TerminalProfileDefinition(
            id: "manual-sudo",
            title: "Manual sudo",
            launch: .sudoUser(unixUser: "operator-target"),
            defaultWorkingDirectory: "/Users/operator-target",
            presentation: nil
        )
        do {
            try store.save(
                TerminalProfileDocument(
                    defaultProfileID: manualProfile.id,
                    profiles: [manualProfile]
                )
            )
        } catch {
            fail("current Managed User test setup must save manual profile: \(error)")
        }

        let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
        let executor = ManagedTerminalAccountHelperExecutor(
            channel: .dev,
            helperClient: helper,
            localEffectExecutor: ManagedTerminalAccountTerminalProfileEffectExecutor(store: store)
        )
        let apply = executor.apply(plan)
        expect(apply.failedStep == nil, "current helper-backed Managed User plan must apply")
        let appliedDocument = store.load().document
        expect(
            appliedDocument.profile(id: "alan")?.launch == .managedUser(unixUser: "alan"),
            "new Managed User profiles must use managed_user"
        )
        expect(
            appliedDocument.profile(id: manualProfile.id) == manualProfile,
            "manually authored sudo_user profiles must remain operator-owned and unchanged"
        )

        let retiredManagedProfile = TerminalProfileDefinition(
            id: "alan",
            title: "Alan Terminal",
            launch: .sudoUser(unixUser: "alan"),
            defaultWorkingDirectory: "/Users/alan",
            presentation: nil,
            managedTerminalAccountID: "alan"
        )
        let retiredDocument = TerminalProfileDocument(
            defaultProfileID: retiredManagedProfile.id,
            profiles: [retiredManagedProfile]
        )
        let readyDiagnosis = AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .alanManaged,
            readinessState: .ready,
            accountExists: true,
            isAdmin: false,
            homeDirectoryExists: true,
            homeDirectoryMatches: true,
            shellMatches: true,
            hiddenFromLoginWindow: true,
            terminalProfileID: request.terminalProfileID,
            ptySmokeVerified: true,
            diagnostic: nil
        )
        let repair = ManagedTerminalAccountPlanner.plan(
            request: request,
            diagnosis: readyDiagnosis,
            terminalProfiles: retiredDocument
        )
        expect(
            repair.steps.map(\.kind).contains(.createOrUpdateTerminalProfile),
            "retired managed sudo_user input must require explicit canonical profile repair"
        )
        expect(
            retiredDocument.profile(id: "alan")?.launch == .sudoUser(unixUser: "alan"),
            "planning must not rewrite retired managed profiles as a load-time migration"
        )

        let rollback = ManagedTerminalAccountPlanner.rollbackPlan(
            request: request,
            diagnosis: readyDiagnosis,
            scope: .alanIntegrationOnly,
            terminalProfiles: appliedDocument
        )
        expect(
            rollback.steps.map(\.kind) == [
                .removeManagedTerminalProfile,
                .helperStep(.removeManagedUserIntegration),
            ],
            "ordinary rollback must remove only current profile and helper-owned integration"
        )
        let destructive = ManagedTerminalAccountPlanner.rollbackPlan(
            request: request,
            diagnosis: readyDiagnosis,
            scope: .deleteAccountAndHome(confirmation: nil)
        )
        expect(
            destructive.status == .requiresDestructiveConfirmation,
            "account and home deletion must require a separate confirmation"
        )
    }

    private static func verifiesBootProfileCacheMemoizesPerLaunchKey() {
        var computeCount = 0
        let cache = AlanShellBootProfileCache(compute: { pane, state in
            computeCount += 1
            return AlanShellBootProfile.forPane(pane, shellState: state)
        })
        let state = ShellStateSnapshot.bootstrapDefault(windowID: "boot_profile_cache_test")
        let pane = ShellPane(
            paneID: "pane_cache",
            tabID: "tab_cache",
            spaceID: "space_cache",
            launchTarget: .shell,
            cwd: "/tmp",
            process: nil,
            attention: .idle,
            context: nil,
            viewport: nil,
            alanBinding: nil,
            terminalProfileID: nil
        )

        _ = cache.profile(for: pane, shellState: state)
        _ = cache.profile(for: pane, shellState: state)
        expect(
            computeCount == 1,
            "boot profile cache must compute once for repeated identical panes (got \(computeCount))"
        )

        let metadataChurnedPane = ShellPane(
            paneID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            launchTarget: pane.launchTarget,
            cwd: pane.cwd,
            process: pane.process,
            attention: .awaitingUser,
            context: pane.context,
            viewport: ShellViewportSnapshot(
                title: "busy",
                summary: "lots of output",
                visibleExcerpt: nil,
                lastActivityAt: "2026-06-15T00:00:00Z"
            ),
            alanBinding: pane.alanBinding,
            terminalProfileID: pane.terminalProfileID
        )
        _ = cache.profile(for: metadataChurnedPane, shellState: state)
        expect(
            computeCount == 1,
            "metadata churn (attention/viewport) must not recompute boot profile (got \(computeCount))"
        )

        // A working-directory change (e.g. `cd`) must NOT redo the expensive
        // resolution — Ghostty discovery and the profile document load are
        // cwd-independent. The new cwd is applied as a cheap overlay.
        let relaunchedPane = ShellPane(
            paneID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            launchTarget: pane.launchTarget,
            cwd: "/var/data",
            process: pane.process,
            attention: pane.attention,
            context: pane.context,
            viewport: pane.viewport,
            alanBinding: pane.alanBinding,
            terminalProfileID: pane.terminalProfileID
        )
        let relaunchedProfile = cache.profile(for: relaunchedPane, shellState: state)
        expect(
            computeCount == 1,
            "a cwd change must not recompute the boot profile (got \(computeCount))"
        )
        expect(
            relaunchedProfile.workingDirectory == "/var/data",
            "a cwd change must be reflected in the boot profile working directory "
                + "(got \(relaunchedProfile.workingDirectory))"
        )
    }

    private static func verifiesSelectedRuntimeAssignmentIgnoresTimestampOnlyChanges() {
        let controller = makeController()
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        func snapshot(at timestamp: TimeInterval) -> TerminalHostRuntimeSnapshot {
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: pane.terminalContentID,
                paneID: pane.paneID,
                tabID: pane.tabID,
                renderPriority: .foregroundInteractive,
                logicalSize: .zero,
                backingSize: .zero,
                displayName: "Studio Display",
                displayID: "display_1",
                attachedWindowTitle: "alan",
                isFocused: false,
                renderer: .placeholder,
                paneMetadata: TerminalPaneMetadataSnapshot(
                    title: "cargo test",
                    workingDirectory: "/repo/app",
                    summary: "build running",
                    attention: .active,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(timeIntervalSince1970: 5)
                ),
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: timestamp)
            )
        }

        var changeCount = 0
        let cancellable = controller.objectWillChange.sink { _ in changeCount += 1 }
        defer { cancellable.cancel() }

        controller.updateTerminalRuntime(snapshot(at: 1))
        let afterFirst = changeCount
        expect(afterFirst > 0, "first runtime publish must notify shell observers")

        // Identical snapshot except for the outer publish timestamp: this is the
        // per-refresh churn that must not invalidate the whole observing tree.
        controller.updateTerminalRuntime(snapshot(at: 2))
        expect(
            changeCount == afterFirst,
            "runtime snapshot identical except for its timestamp must not re-notify "
                + "shell observers (delta \(changeCount - afterFirst))"
        )
    }

    private static func verifiesTerminalCallbackPathDoesNotWriteManifestSynchronously() {
        let writer = SpyPersistenceWriter()
        let scheduler = ManualManifestFlushScheduler()
        let windowID = "manifest_io_test_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID).json")
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: manifestURL),
            persistenceWriter: writer,
            manifestFlushScheduler: scheduler
        )
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        // A burst of metadata callbacks, alternating active-task state to exercise
        // the path that previously wrote the manifest synchronously.
        let states: [ShellTabActiveTaskState] = [.foregroundCommand, .alanRunning]
        for index in 0..<8 {
            controller.updateTerminalMetadata(
                TerminalPaneMetadataSnapshot(
                    title: "cargo test",
                    workingDirectory: "/repo/app",
                    summary: "burst \(index)",
                    attention: .active,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(timeIntervalSince1970: Double(index)),
                    activeTaskState: states[index % states.count]
                ),
                for: pane.paneID
            )
        }

        expect(
            writer.syncWrites == 0,
            "terminal metadata callbacks must not write the manifest "
                + "synchronously (got \(writer.syncWrites) synchronous writes)"
        )
    }

    private static func makeManifestIOController(
        writer: SpyPersistenceWriter,
        scheduler: ManifestFlushScheduling
    ) -> ShellHostController {
        let windowID = "manifest_io_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID).json")
        return makeController(
            windowID: windowID,
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: manifestURL),
            persistenceWriter: writer,
            manifestFlushScheduler: scheduler
        )
    }

    private static func verifiesTerminalCallbackBurstCoalescesIntoOnePersistenceWrite() {
        let writer = SpyPersistenceWriter()
        let scheduler = ManualManifestFlushScheduler()
        let controller = makeManifestIOController(writer: writer, scheduler: scheduler)
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        for index in 0..<10 {
            controller.updateTerminalMetadata(
                TerminalPaneMetadataSnapshot(
                    title: "cargo test",
                    workingDirectory: "/repo/app",
                    summary: "burst \(index)",
                    attention: .active,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(timeIntervalSince1970: Double(index)),
                    activeTaskState: .foregroundCommand
                ),
                for: pane.paneID
            )
        }

        expect(
            scheduler.scheduleCount == 1,
            "a burst of terminal callbacks must schedule one debounced flush "
                + "(scheduled \(scheduler.scheduleCount))"
        )
        expect(
            writer.asyncWrites == 0 && writer.syncWrites == 0,
            "nothing must be written before the debounce window fires"
        )

        scheduler.fire()

        expect(
            writer.manifestWrites == 1,
            "the coalesced flush must write the workspace manifest exactly once "
                + "(manifest \(writer.manifestWrites))"
        )
        expect(
            writer.syncWrites == 0,
            "the coalesced flush must write off the main thread, not synchronously"
        )
    }

    private static func verifiesPaneSupportDirectoryIsCreatedPromptlyAtBoot() {
        let windowID = "pane_dir_boot_\(UUID().uuidString)"
        // Manual scheduler: the debounced/deferred persistence never fires, so any
        // boot-critical pane setup must happen on the prompt (in-memory) path.
        let controller = makeController(
            windowID: windowID,
            manifestFlushScheduler: ManualManifestFlushScheduler()
        )
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }
        let paneURL = alanShellPaneSupportDirectoryURL(windowID: windowID, paneID: pane.paneID)
        defer { try? FileManager.default.removeItem(at: alanShellControlPlaneRootURL(windowID: windowID)) }

        expect(
            FileManager.default.fileExists(atPath: paneURL.path),
            "a pane's support directory must be created promptly at boot, before the debounced flush, "
                + "so a fast shell integration can write its binding file"
        )
    }

    private static func verifiesAsyncPersistenceWriteFailureRoutesToDiagnostics() {
        let windowID = "manifest_async_fail_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID).json")
        // Inject the real persistence writer and verify the controller wires its
        // async error sink to control-plane diagnostics. (This exercises the
        // wiring/routing closure; the param-vs-resolved-writer selection itself
        // is only observable with the internal default writer, which is verified
        // by inspection.)
        let writer = ShellPersistenceWriter(
            manifestStore: ShellWorkspaceManifestStore(manifestURL: manifestURL)
        )
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: manifestURL),
            persistenceWriter: writer
        )

        writer.onError("workspace manifest async save failed")

        expect(
            controller.controlPlaneDiagnostics.contains {
                $0.contains("workspace manifest async save failed")
            },
            "async persistence-write failures must be routed to control-plane diagnostics"
        )
    }

    private static func verifiesFailedStructuralManifestSaveIsReported() {
        let windowID = "manifest_fail_\(UUID().uuidString)"
        let manifestURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(windowID).json")
        let controller = makeController(
            windowID: windowID,
            workspaceManifestStore: ShellWorkspaceManifestStore(manifestURL: manifestURL),
            persistenceWriter: FailingPersistenceWriter(),
            manifestFlushScheduler: ImmediateManifestFlushScheduler()
        )

        // A structural mutation persists synchronously; when the write fails the
        // controller must surface it rather than silently appear to succeed.
        _ = controller.splitPane(paneID: "pane_1", placement: .down)

        expect(
            controller.controlPlaneDiagnostics.contains { $0.contains("workspace manifest save failed") },
            "a failed structural manifest save must record a control-plane diagnostic"
        )
    }

    private static func verifiesLifecycleFlushPersistsPendingContentSynchronously() {
        let writer = SpyPersistenceWriter()
        let scheduler = ManualManifestFlushScheduler()
        let controller = makeManifestIOController(writer: writer, scheduler: scheduler)
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        controller.updateTerminalMetadata(
            TerminalPaneMetadataSnapshot(
                title: "cargo test",
                workingDirectory: "/repo/app",
                summary: "pending output",
                attention: .active,
                processExited: false,
                lastCommandExitCode: nil,
                lastUpdatedAt: Date(timeIntervalSince1970: 1),
                activeTaskState: .foregroundCommand
            ),
            for: pane.paneID
        )
        expect(
            writer.syncWrites == 0 && writer.asyncWrites == 0,
            "a terminal callback alone must not write before the debounce fires"
        )

        controller.flushWorkspacePersistence()

        expect(
            writer.syncWrites >= 1,
            "lifecycle flush must synchronously persist the workspace manifest "
                + "(synchronous writes \(writer.syncWrites))"
        )
    }

    private static func verifiesMetadataCallbackReusesCachedBootProfile() {
        var computeCount = 0
        let cache = AlanShellBootProfileCache(compute: { pane, state in
            computeCount += 1
            return AlanShellBootProfile.forPane(pane, shellState: state)
        })
        let controller = makeController(bootProfileCache: cache)
        guard let pane = controller.selectedPane else {
            fail("bootstrap shell must expose a selected pane")
        }

        func deliverMetadata(burst: Int) {
            controller.updateTerminalMetadata(
                TerminalPaneMetadataSnapshot(
                    title: "cargo test",
                    workingDirectory: "/repo/app",
                    summary: "output burst \(burst)",
                    attention: .active,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(timeIntervalSince1970: Double(burst))
                ),
                for: pane.paneID
            )
        }

        // The first callbacks settle the pane's launch-relevant fields (e.g.
        // cwd from the reported working directory), which legitimately compute
        // the boot profile. Those settle within the warm-up window.
        for burst in 0..<3 {
            deliverMetadata(burst: burst)
        }
        let warmComputeCount = computeCount
        expect(
            warmComputeCount >= 1,
            "metadata callbacks must route boot-profile resolution through the cache"
        )

        // Steady state: a high-output burst of identical callbacks must not
        // recompute the boot profile (no Ghostty discovery / profile disk read).
        for burst in 3..<20 {
            deliverMetadata(burst: burst)
        }
        expect(
            computeCount == warmComputeCount,
            "steady-state metadata callbacks must reuse the cached boot profile "
                + "(recomputed \(computeCount - warmComputeCount) extra times across 17 callbacks)"
        )
    }

    private static func makeController(
        windowID: String = "metadata_test_\(UUID().uuidString)",
        shellState: ShellStateSnapshot? = nil,
        terminalRuntimeRegistry: TerminalRuntimeRegistry? = nil,
        workspaceManifestStore: ShellWorkspaceManifestStore? = nil,
        workspaceManifest: ShellContentWorkspaceManifest? = nil,
        closeConfirmationPresenter: ShellCloseConfirmationPresenting? = nil,
        gracefulShutdownTimeout: TimeInterval = 3.0,
        performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil,
        bootProfileCache: AlanShellBootProfileCache? = nil,
        persistenceWriter: ShellPersistenceWriting? = nil,
        manifestFlushScheduler: ManifestFlushScheduling? = nil,
        pasteboard: ShellPasteboardAccessing? = nil,
        appIsActive: Bool = true
    ) -> ShellHostController {
        let registry =
            terminalRuntimeRegistry
            ?? TerminalRuntimeRegistry(runtimeService: FakeAlanTerminalRuntimeService())
        let context = ShellWindowContext.make(
            windowID: windowID,
            terminalRuntimeRegistry: registry
        )
        let resolvedWriter =
            persistenceWriter
            ?? SynchronousPersistenceWriter(
                manifestStore: workspaceManifestStore
            )
        let resolvedScheduler = manifestFlushScheduler ?? ImmediateManifestFlushScheduler()
        return ShellHostController(
            shellState: shellState ?? .bootstrapDefault(windowID: windowID),
            windowContext: context,
            terminalRuntimeRegistry: registry,
            workspaceManifestStore: workspaceManifestStore,
            workspaceManifest: workspaceManifest,
            persistenceWriter: resolvedWriter,
            manifestFlushScheduler: resolvedScheduler,
            pasteboard: pasteboard,
            closeConfirmationPresenter: closeConfirmationPresenter,
            gracefulShutdownTimeout: gracefulShutdownTimeout,
            performanceDiagnosticsRecorder: performanceDiagnosticsRecorder,
            bootProfileCache: bootProfileCache,
            appIsActiveProvider: { appIsActive }
        )
    }

    final class SpyPersistenceWriter: ShellPersistenceWriting {
        private(set) var syncWrites = 0
        private(set) var asyncWrites = 0
        private(set) var manifestWrites = 0
        private(set) var lastManifest: ShellContentWorkspaceManifest?

        @discardableResult
        func writeManifestSync(_ manifest: ShellContentWorkspaceManifest) -> Bool {
            syncWrites += 1
            manifestWrites += 1
            lastManifest = manifest
            return true
        }

        func writeManifestAsync(_ manifest: ShellContentWorkspaceManifest) {
            asyncWrites += 1
            manifestWrites += 1
            lastManifest = manifest
        }

    }

    /// Default test writer: persists synchronously to the real stores so the
    /// majority of tests observe persistence inline. The off-main/debounced
    /// contract is exercised explicitly by tests that inject the spy + manual
    /// scheduler instead.
    final class SynchronousPersistenceWriter: ShellPersistenceWriting {
        private let manifestStore: ShellWorkspaceManifestStore?

        init(manifestStore: ShellWorkspaceManifestStore?) {
            self.manifestStore = manifestStore
        }

        @discardableResult
        func writeManifestSync(_ manifest: ShellContentWorkspaceManifest) -> Bool {
            do {
                try manifestStore?.save(manifest)
                return true
            } catch {
                return false
            }
        }

        func writeManifestAsync(_ manifest: ShellContentWorkspaceManifest) { try? manifestStore?.save(manifest) }
    }

    final class ImmediateManifestFlushScheduler: ManifestFlushScheduling {
        func schedule(_ work: @escaping () -> Void) { work() }
    }

    final class FailingPersistenceWriter: ShellPersistenceWriting {
        @discardableResult
        func writeManifestSync(_ manifest: ShellContentWorkspaceManifest) -> Bool { false }
        func writeManifestAsync(_ manifest: ShellContentWorkspaceManifest) {}
    }

    final class ManualManifestFlushScheduler: ManifestFlushScheduling {
        private(set) var scheduleCount = 0
        private var pending: (() -> Void)?

        func schedule(_ work: @escaping () -> Void) {
            scheduleCount += 1
            pending = work
        }

        @discardableResult
        func fire() -> Bool {
            guard let work = pending else { return false }
            pending = nil
            work()
            return true
        }
    }

    private static func fakeSurfaceHandle(
        for paneID: String,
        controller: ShellHostController
    ) -> FakeAlanTerminalSurfaceHandle {
        guard let pane = controller.pane(paneID: paneID),
              let handle = controller.terminalRuntimeRegistry.surfaceHandle(
                for: pane,
                bootProfile: controller.bootProfile(for: pane)
              ) as? FakeAlanTerminalSurfaceHandle
        else {
            fail("test setup must create a fake terminal surface handle for \(paneID)")
        }
        return handle
    }

    private static func controlPlaneTestResponse(
        requestID: String,
        applied: Bool
    ) -> AlanShellControlResponse {
        AlanShellControlResponse(
            requestID: requestID,
            contractVersion: ShellContentStateSnapshot.currentContractVersion,
            applied: applied,
            state: nil,
            spaces: nil,
            tabs: nil,
            panes: nil,
            pane: nil,
            items: nil,
            candidates: nil,
            events: nil,
            focusedPaneID: nil,
            spaceID: nil,
            tabID: nil,
            paneID: nil,
            acceptedBytes: nil,
            deliveryCode: nil,
            runtimePhase: nil,
            latestEventID: nil,
            errorCode: nil,
            errorMessage: nil
        )
    }

    private final class RecordingTerminalPasteboardWriter: AlanTerminalPasteboardWriting {
        private(set) var string: String?

        func writeString(_ text: String) -> Bool {
            string = text
            return true
        }
    }

    private final class FakeShellPasteboard: ShellPasteboardAccessing {
        private(set) var string: String?

        init(string: String? = nil) {
            self.string = string
        }

        func readString() -> String? {
            string
        }

        func writeString(_ value: String) {
            string = value
        }
    }

    private final class FakeShellCloseConfirmationPresenter: ShellCloseConfirmationPresenting {
        private var nextResponses: [Bool]
        private(set) var impacts: [ShellCloseGuardImpact] = []

        init(nextResponses: [Bool]) {
            self.nextResponses = nextResponses
        }

        func confirmClose(impact: ShellCloseGuardImpact) -> Bool {
            impacts.append(impact)
            return nextResponses.isEmpty ? false : nextResponses.removeFirst()
        }
    }

    private static func manifestURL(_ prefix: String) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("\(prefix)-\(UUID().uuidString)-workspace.json")
    }

    private static func restoredTranscriptSnapshot(
        contentID: String,
        cwd: String,
        title: String,
        lines: [String]
    ) -> TerminalTranscriptSnapshot {
        TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: cwd,
            title: title,
            dimensions: TerminalTranscriptDimensions(columns: 120, rows: 30),
            viewport: TerminalTranscriptViewport(firstVisibleRow: 0, cursorRow: max(0, lines.count - 1)),
            transcriptLines: lines,
            processSummary: TerminalTranscriptProcessSummary(
                processState: "inactive",
                program: "zsh",
                argvPreview: nil,
                lastCommandExitCode: 0
            ),
            capturedAt: Date(timeIntervalSince1970: 101),
            alternateScreen: false
        )
    }

    private static func contentManifest(
        windowID: String,
        cwd: String,
        transcriptSnapshot: TerminalTranscriptSnapshot?
    ) -> ShellContentWorkspaceManifest {
        ShellContentWorkspaceManifest(
            schemaVersion: ShellContentWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: windowID,
            selectedSpaceID: "space_main",
            selectedTabID: "tab_main",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: Date(timeIntervalSince1970: 100),
                    updatedAt: Date(timeIntervalSince1970: 100),
                    tabs: [
                        ShellContentWorkspaceTabRecord(
                            tabID: "tab_main",
                            title: "Shell",
                            kind: .terminal,
                            createdAt: Date(timeIntervalSince1970: 100),
                            lastActivatedAt: Date(timeIntervalSince1970: 100),
                            lastActivityAt: Date(timeIntervalSince1970: 100),
                            isPinned: false,
                            pinSnapshot: nil,
                            liveSnapshot: contentRestoreSnapshot(
                                paneSlotID: "pane_1",
                                contentID: "content_pane_1",
                                cwd: cwd,
                                transcriptSnapshot: transcriptSnapshot
                            ),
                            activeTask: .inactive
                        )
                    ]
                )
            ]
        )
    }

    private static func decodeManifest(at url: URL) -> ShellContentWorkspaceManifest? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try? decoder.decode(ShellContentWorkspaceManifest.self, from: data)
    }

    private static func rawManifestText(at url: URL) -> String? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private static func terminalPayload(
        in snapshot: ShellContentTabRestoreSnapshot?,
        paneSlotID: String
    ) -> ShellTerminalContentPayload? {
        contentRecord(in: snapshot, paneSlotID: paneSlotID)?.payload.terminal
    }

    private static func roundTripTerminalTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot
    ) -> TerminalTranscriptSnapshot? {
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: 1,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_transcript_roundtrip",
            selectedSpaceID: "space_main",
            selectedTabID: "tab_main",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: Date(timeIntervalSince1970: 90),
                    updatedAt: Date(timeIntervalSince1970: 90),
                    tabs: [
                        ShellContentWorkspaceTabRecord(
                            tabID: "tab_main",
                            title: "Shell",
                            kind: .terminal,
                            createdAt: Date(timeIntervalSince1970: 90),
                            lastActivatedAt: Date(timeIntervalSince1970: 90),
                            lastActivityAt: Date(timeIntervalSince1970: 90),
                            isPinned: false,
                            pinSnapshot: nil,
                            liveSnapshot: contentRestoreSnapshot(
                                paneSlotID: "pane_1",
                                contentID: snapshot.contentID,
                                cwd: snapshot.cwd,
                                transcriptSnapshot: snapshot
                            ),
                            activeTask: .foregroundCommand
                        )
                    ]
                )
            ]
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601

        guard let data = try? encoder.encode(manifest),
              let decoded = try? decoder.decode(ShellContentWorkspaceManifest.self, from: data)
        else {
            fail("terminal transcript snapshot manifest must round-trip")
        }
        return decoded.spaces.first?.tabs.first?.liveSnapshot?.contents.first?
            .payload.terminal?.transcriptSnapshot
    }

    private static func contentRestoreSnapshot(
        paneSlotID: String,
        contentID: String,
        cwd: String?,
        transcriptSnapshot: TerminalTranscriptSnapshot?
    ) -> ShellContentTabRestoreSnapshot {
        ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode(
                nodeID: "node_\(paneSlotID)",
                kind: .pane,
                direction: nil,
                paneSlotID: paneSlotID,
                children: nil
            ),
            paneSlots: [
                ShellPaneSlotRestoreRecord(
                    paneSlotID: paneSlotID,
                    contentID: contentID
                )
            ],
            contents: [
                ShellContentRestoreRecord(
                    contentID: contentID,
                    kind: .terminal,
                    title: "Shell",
                    payload: .terminal(
                        ShellTerminalContentPayload(
                            launchTarget: .shell,
                            cwd: cwd,
                            title: "Shell",
                            transcriptSnapshot: transcriptSnapshot
                        )
                    )
                )
            ]
        )
    }

    private static func contentRecord(
        in snapshot: ShellContentTabRestoreSnapshot?,
        paneSlotID: String
    ) -> ShellContentRestoreRecord? {
        guard let snapshot,
              let paneSlot = snapshot.paneSlots.first(where: { $0.paneSlotID == paneSlotID })
        else {
            return nil
        }
        return snapshot.contents.first { $0.contentID == paneSlot.contentID }
    }

    private static func restoreEnvironmentValue(_ value: String?, forKey key: String) {
        if let value {
            setenv(key, value, 1)
        } else {
            unsetenv(key)
        }
    }

    private static func pane(
        context: ShellContextSnapshot,
        viewport: ShellViewportSnapshot?,
        cwd: String? = "/Users/morris/Developer/Alan",
        launchTarget: ShellLaunchTarget = .shell,
        process: ShellProcessBinding? = ShellProcessBinding(program: "fish", argvPreview: nil),
        attention: ShellAttentionState,
        activity: TerminalActivitySnapshot? = nil,
        terminalProfileID: String? = nil
    ) -> ShellPane {
        ShellPane(
            paneID: "pane_1",
            tabID: "tab_1",
            spaceID: "space_1",
            launchTarget: launchTarget,
            cwd: cwd,
            process: process,
            attention: attention,
            context: context,
            viewport: viewport,
            activity: activity,
            alanBinding: nil,
            terminalProfileID: terminalProfileID
        )
    }

    private static func context(
        workingDirectoryName: String? = "alan",
        repositoryRoot: String? = nil,
        gitBranch: String? = nil,
        processState: String,
        rendererHealth: String,
        surfaceReadiness: String,
        terminalProfileState: String? = nil,
        terminalProfileRequestedID: String? = nil,
        terminalProfileID: String? = nil,
        terminalProfileKind: String? = nil,
        terminalProfileTitle: String? = nil,
        lastCommandExitCode: Int?
    ) -> ShellContextSnapshot {
        ShellContextSnapshot(
            workingDirectoryName: workingDirectoryName,
            repositoryRoot: repositoryRoot,
            gitBranch: gitBranch,
            controlPath: nil,
            alanBindingFile: nil,
            launchStrategy: nil,
            terminalProfileState: terminalProfileState,
            terminalProfileRequestedID: terminalProfileRequestedID,
            terminalProfileID: terminalProfileID,
            terminalProfileKind: terminalProfileKind,
            terminalProfileTitle: terminalProfileTitle,
            shellIntegrationSource: "ghostty_shell_integration",
            processState: processState,
            rendererHealth: rendererHealth,
            surfaceReadiness: surfaceReadiness,
            inputReady: surfaceReadiness == "ready",
            readonly: false,
            terminalMode: "normal_buffer",
            lastMetadataAt: nil,
            lastCommandExitCode: lastCommandExitCode
        )
    }

    private static func metadata(
        title: String,
        cwd: String = "/Users/morris/Developer/Alan",
        processExited: Bool = false,
        activeTaskState: ShellTabActiveTaskState? = nil,
        activity: TerminalActivitySnapshot? = nil,
        clearsActivity: Bool = false
    ) -> TerminalPaneMetadataSnapshot {
        TerminalPaneMetadataSnapshot(
            title: title,
            workingDirectory: cwd,
            summary: nil,
            attention: .idle,
            processExited: processExited,
            lastCommandExitCode: nil,
            lastUpdatedAt: Date(timeIntervalSince1970: 3_000),
            activeTaskState: activeTaskState,
            activity: activity,
            clearsActivity: clearsActivity
        )
    }

    private static func stateWithContext(
        windowID: String,
        context: ShellContextSnapshot,
        spaceTerminalProfileID: String? = nil,
        spacePresentationIconSystemName: String? = nil,
        paneTerminalProfileID: String? = nil
    ) -> ShellStateSnapshot {
        let pane = pane(
            context: context,
            viewport: nil,
            attention: .active,
            terminalProfileID: paneTerminalProfileID
        )
        return ShellStateSnapshot(
            contractVersion: "0.1",
            windowID: windowID,
            focusedSpaceID: "space_1",
            focusedTabID: "tab_1",
            focusedPaneID: "pane_1",
            spaces: [
                ShellSpace(
                    spaceID: "space_1",
                    title: "Main",
                    attention: pane.attention,
                    tabs: [
                        ShellTab(
                            tabID: "tab_1",
                            kind: .terminal,
                            title: "alan",
                            paneTree: ShellPaneTreeNode(
                                nodeID: "node_pane_1",
                                kind: .pane,
                                direction: nil,
                                paneID: "pane_1",
                                children: nil
                            )
                        )
                    ],
                    terminalProfileID: spaceTerminalProfileID,
                    presentationIconSystemName: spacePresentationIconSystemName
                )
            ],
            panes: [pane]
        )
    }

    private static func progressActivity(
        percent: Int,
        updatedAt: String,
        staleAt: String
    ) -> TerminalActivitySnapshot {
        activity(
            status: .progress,
            source: .progress,
            sourceLabel: "Progress",
            stateLabel: "\(percent)%",
            progress: .percent(percent),
            updatedAt: updatedAt,
            staleAt: staleAt
        )
    }

    private static func activity(
        status: TerminalActivityStatus,
        source: TerminalActivitySourceKind,
        sourceLabel: String,
        stateLabel: String,
        detailLabel: String? = nil,
        progress: TerminalActivityProgress? = nil,
        command: TerminalActivityCommandOutcome? = nil,
        agent: TerminalActivityAgentMetadata? = nil,
        updatedAt: String = "2026-05-17T09:00:00Z",
        staleAt: String? = nil
    ) -> TerminalActivitySnapshot {
        TerminalActivitySnapshot(
            source: .init(kind: source, label: sourceLabel),
            status: status,
            priority: priority(for: status),
            progress: progress,
            command: command,
            agent: agent,
            display: TerminalActivityDisplay(
                sourceLabel: sourceLabel,
                stateLabel: stateLabel,
                detailLabel: detailLabel,
                paneHint: nil
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: updatedAt,
                staleAt: staleAt,
                expiresAt: nil
            )
        )
    }

    private static func commandActivity(
        exitCode: Int,
        durationMilliseconds: Int,
        updatedAt: String
    ) -> TerminalActivitySnapshot {
        let succeeded = exitCode == 0
        return activity(
            status: succeeded ? .done : .failed,
            source: .command,
            sourceLabel: "Shell",
            stateLabel: succeeded ? "Command succeeded" : "Command failed \(exitCode)",
            command: TerminalActivityCommandOutcome(
                exitCode: exitCode,
                durationMilliseconds: durationMilliseconds,
                commandText: nil
            ),
            updatedAt: updatedAt
        )
    }

    private static func priority(for status: TerminalActivityStatus) -> TerminalActivityPriority {
        switch status {
        case .needsInput:
            return .awaitingUser
        case .failed, .exited:
            return .notable
        case .paused, .progress, .running, .bell:
            return .active
        case .idle, .done, .stale:
            return .passive
        }
    }

    private static func childExitMetadata(
        title: String,
        cwd: String = "/Users/morris/Developer/Alan",
        exitCode: Int,
        activity: TerminalActivitySnapshot? = nil
    ) -> TerminalPaneMetadataSnapshot {
        TerminalPaneMetadataSnapshot(
            title: title,
            workingDirectory: cwd,
            summary: "process exited",
            attention: .awaitingUser,
            processExited: true,
            lastCommandExitCode: exitCode,
            lastUpdatedAt: Date(timeIntervalSince1970: 3_100),
            activeTaskState: .inactive,
            activity: activity
        )
    }

    private static func activeOnlyMetadata(
        activeTaskState: ShellTabActiveTaskState
    ) -> TerminalPaneMetadataSnapshot {
        TerminalPaneMetadataSnapshot(
            title: nil,
            workingDirectory: nil,
            summary: nil,
            attention: .idle,
            processExited: false,
            lastCommandExitCode: nil,
            lastUpdatedAt: nil,
            activeTaskState: activeTaskState
        )
    }

    private static func stateWithAlanBinding(
        windowID: String,
        pendingRequest: Bool,
        activity: TerminalActivitySnapshot? = nil
    ) -> ShellStateSnapshot {
        let pane = ShellPane(
            paneID: "pane_1",
            tabID: "tab_main",
            spaceID: "space_main",
            launchTarget: .shell,
            cwd: "/tmp",
            process: ShellProcessBinding(program: "alan", argvPreview: ["alan", "chat"]),
            attention: pendingRequest ? .awaitingUser : .active,
            context: ShellContextSnapshot(
                workingDirectoryName: "tmp",
                repositoryRoot: nil,
                gitBranch: nil,
                controlPath: "/tmp/control",
                alanBindingFile: "/tmp/binding",
                launchStrategy: "login_shell",
                shellIntegrationSource: "ghostty_shell_integration",
                processState: "running",
                lastMetadataAt: nil,
                lastCommandExitCode: nil
            ),
            viewport: nil,
            activity: activity,
            alanBinding: ShellAlanBinding(
                processPath: "/proc/1",
                machineState: pendingRequest ? "waiting" : "running",
                pendingRequest: pendingRequest,
                source: "test",
                lastProjectedAt: nil
            )
        )

        return ShellStateSnapshot(
            contractVersion: "0.1",
            windowID: windowID,
            focusedSpaceID: "space_main",
            focusedTabID: "tab_main",
            focusedPaneID: "pane_1",
            spaces: [
                ShellSpace(
                    spaceID: "space_main",
                    title: "Main",
                    attention: pane.attention,
                    tabs: [
                        ShellTab(
                            tabID: "tab_main",
                            kind: .terminal,
                            title: "alan",
                            paneTree: ShellPaneTreeNode(
                                nodeID: "node_pane_1",
                                kind: .pane,
                                direction: nil,
                                paneID: "pane_1",
                                children: nil
                            )
                        )
                    ]
                )
            ],
            panes: [pane]
        )
    }

    private static func activeTask(in url: URL) -> ShellTabActiveTaskState? {
        decodeManifest(at: url)?.spaces.first?.tabs.first?.activeTask
    }

    @MainActor


    private static func decodeControlCommand(_ json: String) -> AlanShellControlCommand {
        do {
            let data = Data(json.utf8)
            return try JSONDecoder().decode(AlanShellControlCommand.self, from: data)
        } catch {
            fail("failed to decode control command fixture: \(error)")
        }
    }

    private static func jsonEscaped(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
    }

    private static func controlEvents(_ controller: ShellHostController) -> [AlanShellEventEnvelope] {
        controller.handleControlPlaneCommand(
            decodeControlCommand(
                """
                {
                  "request_id": "events-read-\(UUID().uuidString)",
                  "command": "events.read",
                  "limit": 200
                }
                """
            )
        ).events ?? []
    }

    private static func temporaryDirectory(named name: String) -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(name)-\(UUID().uuidString)", isDirectory: true)
        try! FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private static func terminalPayload(
        in snapshot: ShellContentTabRestoreSnapshot?,
        contentID: String
    ) -> ShellTerminalContentPayload? {
        snapshot?.contents.first { $0.contentID == contentID }?.payload.terminal
    }

    private static func defaultManifestWithShellCore(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellContentWorkspaceManifest {
        do {
            return try ShellCoreFFIAdapter().defaultContentWorkspaceManifest(
                windowID: windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
        } catch {
            fail("shell-core default manifest failed: \(error)")
        }
    }

    private static func materializeManifestWithShellCore(
        manifest: ShellContentWorkspaceManifest,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellStateSnapshot {
        do {
            return try ShellCoreFFIAdapter().materializeContentWorkspaceManifest(
                manifest: manifest,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
        } catch {
            fail("shell-core manifest materialize failed: \(error)")
        }
    }

    private static func expect(
        _ condition: @autoclosure () -> Bool,
        _ message: String
    ) {
        guard condition() else {
            fail(message)
        }
    }

    private static func fail(_ message: String) -> Never {
        fputs("error: \(message)\n", stderr)
        exit(1)
    }

    private static func verifiesShellResolverFallsBackToLoginShellWhenCoreUnavailable() {
        let environmentKey = "ALAN_SHELL_CORE_FFI_LIBRARY"
        let originalPath = ProcessInfo.processInfo.environment[environmentKey]
        let missingPath = "/tmp/alan-shell-core-ffi-missing-\(UUID().uuidString.lowercased()).dylib"
        setenv(environmentKey, missingPath, 1)
        defer {
            if let originalPath {
                setenv(environmentKey, originalPath, 1)
            } else {
                unsetenv(environmentKey)
            }
        }

        // No explicit profile: shell-core cannot load, but a default terminal must still resolve a
        // usable login shell instead of the immediately-exiting shell-core failure command.
        let resolution = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: nil,
            terminalProfiles: nil
        )
        expect(
            resolution.summary != "Terminal Profile unavailable",
            "default terminal must not resolve to the shell-core failure command when core is unavailable"
        )
        expect(
            !(resolution.surfaceCommand?.contains("exit 78") ?? false),
            "default-terminal fallback must launch a login shell, not an immediately-exiting command"
        )

        let explicitProfileResolution = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: "profile-admin",
            terminalProfiles: nil
        )
        expect(
            explicitProfileResolution.summary == "Terminal Profile unavailable",
            "explicit Terminal Profile resolution must fail closed when shell-core is unavailable"
        )
        expect(
            explicitProfileResolution.surfaceCommand?.contains("exit 78") == true,
            "explicit Terminal Profile fallback must keep the shell-core failure command"
        )
    }

    private final class AlwaysExecutableFileManager: FileManager {
        override func fileExists(atPath path: String) -> Bool {
            true
        }

        override func isExecutableFile(atPath path: String) -> Bool {
            true
        }
    }


}
#endif
