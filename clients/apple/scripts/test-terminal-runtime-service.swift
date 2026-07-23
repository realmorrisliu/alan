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
        verifiesSourceTreeRepositoryRootInference()
        verifiesControlSequenceResponderAnswersPrimaryDeviceAttributes()
        verifiesControlSequenceResponderReportsShellActivity()
        verifiesGhosttyTerminfoEnvironmentProjection()
        verifiesBootProfileExposesStructuredBootRequest()
        verifiesManagedUserLaunchResolutionUsesHelperIdentityWithoutSudo()
        verifiesFakePtyRuntimeCapturesLaunchAndLifecycle()
        verifiesManagedUserPtyRuntimeFailsClosedWithoutSudoFallback()
        verifiesManagedUserPtyRuntimeUsesHelperProviderWhenAvailable()
        verifiesWindowRuntimeDefaultPtyRuntimeWiresHelperProvider()
        verifiesManagedUserSurfaceRoutesHelperPtyLifecycleControls()
        verifiesManagedUserDirectDrainReportsShellActivity()
        verifiesManagedUserRendererAttachmentBridgesHelperSession()
        verifiesAlanGhosttySurfaceDeliveryUsesPtyRuntimeWithoutRenderer()
        verifiesDarwinPtyBackendLaunchesLocalShell()
        verifiesDarwinPtyBackendKeepsLoginShellAlive()
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

    private static func verifiesSourceTreeRepositoryRootInference() {
        expect(
            inferredAlanRepoRoot(
                from: "/tmp/alan/clients/apple/alan-macos/Services/Terminal/TerminalBootResolution.swift"
            ) == "/tmp/alan",
            "repository root inference must remain correct when its source owner moves"
        )
        expect(
            inferredAlanRepoRoot(from: "/tmp/TerminalBootResolution.swift") == nil,
            "repository root inference must fail closed outside the canonical clients/apple tree"
        )
    }

    private static func verifiesControlSequenceResponderAnswersPrimaryDeviceAttributes() {
        var responder = AlanTerminalPtyControlSequenceResponder()

        var response = responder.process(Data("hello \u{1B}[c world".utf8))
        expect(
            response.rendererOutput == Data("hello  world".utf8),
            "primary device attributes query must be consumed before renderer output"
        )
        expect(
            response.ptyResponse == Data("\u{1B}[?62;22c".utf8),
            "primary device attributes query must receive a terminal response"
        )

        response = responder.process(Data("\u{1B}[".utf8))
        expect(
            response.rendererOutput.isEmpty && response.ptyResponse.isEmpty,
            "partial CSI must be buffered across PTY chunks"
        )
        response = responder.process(Data("0c".utf8))
        expect(
            response.rendererOutput.isEmpty,
            "split primary device attributes query must not reach renderer output"
        )
        expect(
            response.ptyResponse == Data("\u{1B}[?62;22c".utf8),
            "split primary device attributes query must receive a terminal response"
        )

        var suppressedResponder = AlanTerminalPtyControlSequenceResponder()
        suppressedResponder.suppressNextPrimaryDeviceAttributesResponse()
        response = suppressedResponder.process(Data("\u{1B}[c".utf8))
        expect(
            response.rendererOutput.isEmpty && response.ptyResponse.isEmpty,
            "suppressed primary device attributes query must be consumed without duplicate response"
        )
        response = suppressedResponder.process(Data("\u{1B}[c".utf8))
        expect(
            response.ptyResponse == Data("\u{1B}[?62;22c".utf8),
            "only the preseeded primary device attributes response should be suppressed"
        )

        response = responder.process(Data("\u{1B}[31mred".utf8))
        expect(
            response.rendererOutput == Data("\u{1B}[31mred".utf8),
            "non-DA CSI sequences must continue to Ghostty unchanged"
        )
        expect(response.ptyResponse.isEmpty, "non-DA CSI sequences must not emit PTY responses")

        response = responder.process(Data("\u{1B}[6n".utf8))
        expect(
            response.rendererOutput.isEmpty,
            "cursor position report query must be consumed before renderer output"
        )
        expect(
            response.ptyResponse == Data("\u{1B}[1;1R".utf8),
            "cursor position report query must receive a bounded PTY response"
        )

        response = responder.process(Data("\u{1B}]11;?\u{7}".utf8))
        expect(
            response.rendererOutput.isEmpty,
            "background color query must be consumed before renderer output"
        )
        expect(
            response.ptyResponse == Data("\u{1B}]11;rgb:0a0a/0c0c/1010\u{1B}\\".utf8),
            "background color query must receive a bounded PTY response"
        )

        response = responder.process(Data("\u{1B}]".utf8))
        expect(
            response.rendererOutput.isEmpty && response.ptyResponse.isEmpty,
            "partial OSC must be buffered across PTY chunks"
        )
        response = responder.process(Data("11;?\u{1B}\\".utf8))
        expect(
            response.rendererOutput.isEmpty,
            "split background color query must not reach renderer output"
        )
        expect(
            response.ptyResponse == Data("\u{1B}]11;rgb:0a0a/0c0c/1010\u{1B}\\".utf8),
            "split background color query must receive a bounded PTY response"
        )

        response = responder.process(Data("\u{1B}]0;title\u{7}".utf8))
        expect(
            response.rendererOutput == Data("\u{1B}]0;title\u{7}".utf8),
            "non-query OSC sequences must continue to Ghostty unchanged"
        )
        expect(response.ptyResponse.isEmpty, "non-query OSC sequences must not emit PTY responses")
    }

    private static func verifiesControlSequenceResponderReportsShellActivity() {
        var responder = AlanTerminalPtyControlSequenceResponder()

        var response = responder.process(Data("\u{1B}]133;C\u{7}".utf8))
        expect(
            response.shellActivityTransition == .foregroundCommand,
            "OSC 133 C must report foreground command activity"
        )
        expect(
            response.rendererOutput == Data("\u{1B}]133;C\u{7}".utf8),
            "shell activity markers must continue to Ghostty unchanged"
        )

        response = responder.process(Data("stdin line\n".utf8))
        expect(
            response.shellActivityTransition == nil,
            "ordinary newline-delimited PTY input or output must not be treated as a command"
        )

        response = responder.process(Data("\u{1B}]133;D;0\u{7}".utf8))
        expect(
            response.shellActivityTransition == nil,
            "command completion must remain active until shell integration returns to a prompt"
        )

        response = responder.process(Data("\u{1B}]133;A;cl=line\u{7}".utf8))
        expect(
            response.shellActivityTransition == .shellInput,
            "OSC 133 prompt start must report shell input readiness"
        )

        response = responder.process(Data("\u{1B}]133;Cextra\u{7}".utf8))
        expect(
            response.shellActivityTransition == nil,
            "malformed OSC 133 command markers must not change shell activity"
        )

        response = responder.process(
            Data("\u{1B}]133;A\u{7}\u{1B}]133;C;aid=next\u{1B}\\".utf8)
        )
        expect(
            response.shellActivityTransition == .foregroundCommand,
            "the last semantic transition in one PTY chunk must win"
        )

        response = responder.process(Data("\u{1B}]133;".utf8))
        expect(
            response.shellActivityTransition == nil,
            "partial shell integration markers must be buffered across PTY chunks"
        )
        response = responder.process(Data("B\u{7}".utf8))
        expect(
            response.shellActivityTransition == .shellInput,
            "split OSC 133 prompt markers must preserve semantic state"
        )
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
            profile.environment["TERM"] == "xterm-256color",
            "boot profile must provide a terminal type to child processes"
        )
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

    private static func verifiesDarwinPtyBackendLaunchesLocalShell() {
        let command = """
        $| = 1;
        $SIG{INT} = sub { print "signal-int\\n"; exit 130; };
        print "ready\\n";
        my $line = <STDIN>;
        chomp $line;
        print "got:$line\\n";
        while (1) { sleep 1; }
        """
        let request = AlanTerminalBootRequest(
            strategy: .terminalProfileCustomCommand,
            executablePath: "/usr/bin/perl",
            arguments: ["-e", command],
            workingDirectory: "/tmp",
            environment: ["TERM": "xterm-256color"],
            bootCommand: command,
            rendererCompatibilityCommand: nil,
            managedUserAccountName: nil,
            terminalProfile: nil
        )
        let runtime = AlanDarwinTerminalPtyRuntime()
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_darwin_pty",
            bootRequest: request
        ) as! AlanDarwinTerminalPtyHandle
        defer {
            if handle.snapshot.exitStatus == nil {
                _ = handle.sendSignal(.kill)
                _ = waitForDarwinPtyExit(handle)
            }
        }

        expect(
            runtime.registeredContentIDs == ["content_terminal_darwin_pty"],
            "Darwin PTY runtime must register content identity"
        )
        expect(handle.snapshot.phase == .running, "Darwin PTY runtime must start running")
        expect(handle.isInputReady, "Darwin PTY runtime must start input-ready")
        expect(
            waitForDarwinPtyOutput(handle, contains: "ready"),
            "Darwin PTY runtime must capture child output"
        )

        let resize = handle.resize(columns: 100, rows: 30)
        expect(resize.accepted, "Darwin PTY runtime must accept resize")
        expect(
            handle.snapshot.dimensions == AlanTerminalPtyDimensions(columns: 100, rows: 30),
            "Darwin PTY runtime must snapshot resize dimensions"
        )

        let input = handle.writeInput("hello\n")
        expect(input.applied, "Darwin PTY runtime must accept input")
        expect(input.acceptedBytes == 6, "Darwin PTY runtime must report input bytes")
        expect(
            waitForDarwinPtyOutput(handle, contains: "got:hello"),
            "Darwin PTY runtime must capture output after input"
        )

        let interrupt = handle.sendSignal(.interrupt)
        expect(interrupt.accepted, "Darwin PTY runtime must accept interrupt signal")
        expect(handle.snapshot.lastSignal == .interrupt, "Darwin PTY runtime must snapshot last signal")
        let interruptedExit = waitForDarwinPtyExit(handle, timeout: 5)
        if interruptedExit == nil {
            let snapshot = handle.snapshot
            fail(
                """
                Darwin PTY runtime must observe child exit after interrupt \
                signal=\(interrupt.code) phase=\(snapshot.phase.rawValue) \
                exit=\(String(describing: snapshot.exitStatus)) \
                transcript=\(snapshot.transcriptLines.joined(separator: "|"))
                """
            )
        }
        expect(handle.snapshot.phase == .exited, "Darwin PTY runtime must snapshot exited phase")
    }

    private static func verifiesDarwinPtyBackendKeepsLoginShellAlive() {
        let environmentShell = ProcessInfo.processInfo.environment["SHELL"] ?? ""
        let shell = FileManager.default.isExecutableFile(atPath: environmentShell)
            ? environmentShell
            : "/bin/zsh"
        let marker = "alan_login_shell_ready_\(UUID().uuidString.replacingOccurrences(of: "-", with: ""))"
        let request = AlanTerminalBootRequest(
            strategy: .loginShellEnv,
            executablePath: shell,
            arguments: ["-l"],
            workingDirectory: "/tmp",
            environment: ["TERM": "xterm-256color"],
            bootCommand: "\(shell) -l",
            rendererCompatibilityCommand: nil,
            managedUserAccountName: nil,
            terminalProfile: nil
        )
        let runtime = AlanDarwinTerminalPtyRuntime()
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_login_shell_pty",
            bootRequest: request
        ) as! AlanDarwinTerminalPtyHandle
        defer {
            if handle.snapshot.exitStatus == nil {
                _ = handle.writeInput("exit\n")
                if waitForDarwinPtyExit(handle, timeout: 1) == nil {
                    _ = handle.sendSignal(.kill)
                    _ = waitForDarwinPtyExit(handle)
                }
            }
        }

        usleep(250_000)
        expect(handle.snapshot.phase == .running, "login shell PTY runtime must stay running")
        expect(handle.snapshot.exitStatus == nil, "login shell must not exit immediately")

        let input = handle.writeInput("printf '%s\\n' \(marker)\n")
        expect(input.applied, "login shell PTY runtime must accept input")
        expect(
            waitForDarwinPtyOutput(handle, contains: marker, timeout: 4),
            "login shell PTY runtime must execute input over the PTY"
        )
    }

    private static func verifiesBootProfileExposesStructuredBootRequest() {
        let terminalProfile = TerminalProfileDefinition(
            id: "profile_custom",
            title: "Custom",
            launch: .customCommand("echo hi"),
            defaultWorkingDirectory: "/tmp/project",
            presentation: nil
        )
        let command = AlanCommandResolution(
            strategy: .terminalProfileCustomCommand,
            executablePath: "/bin/zsh",
            launchPath: "/bin/zsh",
            arguments: ["-lc", "echo hi"],
            bootCommand: "echo hi",
            surfaceCommand: "echo hi",
            summary: "Launching pane with Terminal Profile Custom",
            detail: "Custom command",
            repoRoot: nil,
            candidates: [],
            terminalProfile: terminalProfile,
            terminalProfileState: .resolved
        )
        let profile = sampleBootProfile(
            workingDirectory: "/tmp/project",
            command: command,
            environment: [
                "ALAN_SHELL_CONTENT_ID": "content_terminal_boot",
                "ALAN_TERMINAL_PROFILE_REQUESTED_ID": "profile_custom",
            ]
        )

        let request = profile.bootRequest

        expect(request.strategy == .terminalProfileCustomCommand, "boot request must preserve launch strategy")
        expect(request.executablePath == "/bin/zsh", "boot request must preserve executable path")
        expect(request.arguments == ["-lc", "echo hi"], "boot request must preserve launch arguments")
        expect(request.workingDirectory == "/tmp/project", "boot request must preserve cwd")
        expect(
            request.environment["ALAN_SHELL_CONTENT_ID"] == "content_terminal_boot",
            "boot request must preserve terminal environment"
        )
        expect(request.bootCommand == "echo hi", "boot request must preserve human-readable boot command")
        expect(
            request.rendererCompatibilityCommand == "echo hi",
            "boot request must preserve the temporary Ghostty renderer command"
        )
        expect(
            profile.surfaceCommand == request.rendererCompatibilityCommand,
            "surfaceCommand must be derived from structured boot request"
        )
        expect(
            profile.launchCommandString == request.launchCommandString,
            "launch command string must be derived from structured boot request"
        )
        expect(
            request.terminalProfile?.requestedID == "profile_custom",
            "boot request must preserve requested Terminal Profile id"
        )
        expect(
            request.terminalProfile?.resolvedID == "profile_custom",
            "boot request must preserve resolved Terminal Profile id"
        )
        expect(
            request.terminalProfile?.kind == TerminalProfileLaunchKind.customCommand.rawValue,
            "boot request must preserve Terminal Profile launch kind"
        )
        expect(
            request.terminalProfile?.state == .resolved,
            "boot request must preserve Terminal Profile resolution state"
        )
    }

    private static func verifiesManagedUserLaunchResolutionUsesHelperIdentityWithoutSudo() {
        let terminalProfile = TerminalProfileDefinition(
            id: "lab",
            title: "Lab User",
            launch: .managedUser(unixUser: "lab"),
            defaultWorkingDirectory: "/Users/lab",
            presentation: nil,
            managedTerminalAccountID: "lab"
        )
        let document = TerminalProfileDocument(
            defaultProfileID: terminalProfile.id,
            profiles: [TerminalProfileDefinition.loginShellFallback, terminalProfile]
        )
        let command = AlanCommandResolution.resolve(
            for: .shell,
            terminalProfileReference: terminalProfile.id,
            terminalProfiles: document,
            environment: ["SHELL": "/bin/zsh"]
        )

        expect(
            command.strategy == .terminalProfileManagedUser,
            "managed_user profiles must resolve to the helper-backed launch strategy"
        )
        expect(command.managedUserAccountName == "lab", "managed_user resolution must carry the account name")
        expect(command.executablePath == nil, "managed_user resolution must not resolve a local executable")
        expect(command.launchPath.isEmpty, "managed_user resolution must not spawn through a launch path")
        expect(command.arguments.isEmpty, "managed_user resolution must not synthesize sudo arguments")
        expect(
            command.launchCommandString == "managed_user 'lab'",
            "managed_user launch string must expose the structured helper identity"
        )
        expect(
            !command.launchCommandString.contains("/usr/bin/sudo")
                && !command.launchCommandString.contains("sudo -"),
            "managed_user launch resolution must not fall back to sudo"
        )

        let bootProfile = sampleBootProfile(
            workingDirectory: "/Users/lab",
            command: command,
            environment: [
                "ALAN_SHELL_CONTENT_ID": "content_terminal_managed_user",
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
                "ALAN_TERMINAL_PROFILE_REQUESTED_ID": "lab",
            ]
        )
        let request = bootProfile.bootRequest
        expect(
            request.strategy == .terminalProfileManagedUser,
            "managed_user boot requests must preserve the helper-backed launch strategy"
        )
        expect(
            request.managedUserAccountName == "lab",
            "managed_user boot requests must preserve the target account"
        )
        expect(
            request.launchCommandString == "managed_user 'lab'",
            "managed_user boot request launch text must stay helper-scoped"
        )
        expect(
            request.rendererCompatibilityCommand == nil,
            "managed_user boot requests must not give Ghostty a fallback launch command"
        )
        expect(
            bootProfile.surfaceCommand == nil,
            "managed_user surface configuration must not expose a Ghostty-owned process command"
        )
        expect(
            request.terminalProfile?.kind == TerminalProfileLaunchKind.managedUser.rawValue,
            "managed_user boot requests must preserve Terminal Profile launch kind"
        )
    }

    private static func verifiesFakePtyRuntimeCapturesLaunchAndLifecycle() {
        let profile = sampleBootProfile(
            workingDirectory: "/tmp/project",
            environment: ["ALAN_SHELL_CONTENT_ID": "content_terminal_pty"]
        )
        let runtime = FakeAlanTerminalPtyRuntime()
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_pty",
            bootRequest: profile.bootRequest
        ) as! FakeAlanTerminalPtyHandle

        expect(
            runtime.registeredContentIDs == ["content_terminal_pty"],
            "fake PTY runtime must register content identity"
        )
        expect(
            handle.bootRequest == profile.bootRequest,
            "fake PTY handle must retain structured boot request"
        )
        expect(handle.isInputReady, "fake PTY runtime must start input-ready")

        let input = handle.writeInput("echo hi\n")
        expect(input.applied, "fake PTY runtime must accept input before exit")
        expect(input.acceptedBytes == 8, "fake PTY runtime must report accepted input bytes")
        expect(handle.deliveredText == ["echo hi\n"], "fake PTY runtime must record delivered input")

        let resize = handle.resize(columns: 120, rows: 32)
        expect(resize.accepted, "fake PTY runtime must accept resize requests")
        expect(
            handle.snapshot.dimensions == AlanTerminalPtyDimensions(columns: 120, rows: 32),
            "fake PTY runtime must snapshot latest dimensions"
        )

        let signal = handle.sendSignal(.interrupt)
        expect(signal.accepted, "fake PTY runtime must accept signal requests")
        expect(handle.snapshot.lastSignal == .interrupt, "fake PTY runtime must snapshot latest signal")

        handle.recordTranscriptOutput("line one\nline two")
        expect(
            handle.snapshot.transcriptLines == ["line one", "line two"],
            "fake PTY runtime must keep bounded transcript lines"
        )

        let eof = handle.closeInput()
        expect(eof.accepted, "fake PTY runtime must accept EOF")
        expect(handle.snapshot.inputClosed, "fake PTY runtime must snapshot EOF")
        expect(!handle.isInputReady, "fake PTY runtime must stop accepting input after EOF")

        handle.markExited(.exitCode(0))
        expect(handle.snapshot.phase == .exited, "fake PTY runtime must snapshot exit phase")
        expect(
            handle.snapshot.exitStatus?.diagnosticsValue == "exit:0",
            "fake PTY runtime must snapshot exit status"
        )

        let rejected = handle.writeInput("after exit")
        expect(!rejected.applied, "fake PTY runtime must reject input after exit")
        expect(
            rejected.errorCode == "terminal_child_exited",
            "fake PTY runtime must use stable exited error code"
        )
    }

    private static func verifiesManagedUserPtyRuntimeFailsClosedWithoutSudoFallback() {
        let request = AlanTerminalBootRequest(
            strategy: .terminalProfileManagedUser,
            executablePath: "",
            arguments: [],
            workingDirectory: "/Users/lab",
            environment: [
                "ALAN_SHELL_CONTENT_ID": "content_terminal_managed_user_unavailable",
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
            ],
            bootCommand: "managed_user 'lab'",
            rendererCompatibilityCommand: nil,
            managedUserAccountName: "lab",
            terminalProfile: nil
        )
        let runtime = AlanDarwinTerminalPtyRuntime()
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_managed_user_unavailable",
            bootRequest: request
        )

        expect(
            handle is AlanUnavailableManagedUserPtyHandle,
            "managed_user launches must fail closed when the helper PTY provider is unavailable"
        )
        expect(
            !(handle is AlanDarwinTerminalPtyHandle),
            "managed_user launches must not fall back to the local Darwin process spawner"
        )
        expect(handle.snapshot.phase == .failed, "unavailable managed_user runtime must publish failed phase")
        expect(!handle.isInputReady, "unavailable managed_user runtime must not accept input")
        expect(
            handle.snapshot.bootRequest.launchCommandString == "managed_user 'lab'",
            "managed_user runtime snapshots must preserve helper-scoped launch metadata"
        )

        let delivery = handle.writeInput("whoami\n")
        expect(delivery.code == .unavailableRuntime, "unavailable managed_user input must report unavailable")
        expect(!delivery.applied, "unavailable managed_user input must not report applied")
        expect(
            delivery.errorCode == "terminal_runtime_unavailable",
            "unavailable managed_user input must use the stable runtime unavailable code"
        )
        expect(
            handle.resize(columns: 100, rows: 30).code == "managed_user_helper_unavailable",
            "unavailable managed_user resize must be rejected through helper diagnostics"
        )
        expect(
            handle.sendSignal(.interrupt).code == "managed_user_helper_unavailable",
            "unavailable managed_user signal routing must not claim success"
        )
        switch handle.makeRendererAttachment() {
        case .attached:
            fail("unavailable managed_user runtime must not expose a renderer PTY attachment")
        case .rejected(let rejection):
            expect(
                rejection.code == "managed_user_helper_unavailable",
                "unavailable managed_user renderer attachment must be rejected with helper diagnostics"
            )
        }
    }

    private static func verifiesManagedUserPtyRuntimeUsesHelperProviderWhenAvailable() {
        let request = AlanTerminalBootRequest(
            strategy: .terminalProfileManagedUser,
            executablePath: "",
            arguments: [],
            workingDirectory: "/Users/lab/project",
            environment: [
                "ALAN_SHELL_CONTENT_ID": "content_terminal_managed_user_helper",
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
            ],
            bootCommand: "managed_user 'lab'",
            rendererCompatibilityCommand: nil,
            managedUserAccountName: "lab",
            terminalProfile: nil
        )
        let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
        let runtime = AlanDarwinTerminalPtyRuntime(
            managedUserPtyProvider: AlanHelperManagedUserPtyProvider(
                helperClient: helper,
                defaultDimensions: AlanTerminalPtyDimensions(columns: 100, rows: 30)
            )
        )
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_managed_user_helper",
            bootRequest: request
        )

        expect(
            handle is AlanHelperManagedUserPtyHandle,
            "managed_user launches must use helper-backed PTY handles when provider is available"
        )
        expect(
            !(handle is AlanDarwinTerminalPtyHandle),
            "helper-backed managed_user launches must not use the Darwin local process spawner"
        )
        expect(
            helper.startedPTYRequests.first?.accountName == "lab"
                && helper.startedPTYRequests.first?.contentID == "content_terminal_managed_user_helper"
                && helper.startedPTYRequests.first?.homeDirectory == "/Users/lab"
                && helper.startedPTYRequests.first?.workingDirectory == "/Users/lab/project"
                && helper.startedPTYRequests.first?.columns == 100
                && helper.startedPTYRequests.first?.rows == 30,
            "helper PTY provider must send typed startManagedUserPTY requests with canonical account home and pane cwd"
        )
        expect(handle.snapshot.phase == .running, "helper-backed managed_user handle must snapshot running")
        expect(
            handle.snapshot.transcriptLines.contains("Fake helper PTY session started."),
            "helper-backed managed_user handle must expose sanitized helper startup diagnostics"
        )
        let delivery = handle.writeInput("whoami\n")
        expect(delivery.applied, "managed_user input must route through helper PTY sessions")
        expect(
            helper.writtenPTYInputRequests.first
                == AlanManagedUserPTYInputRequest(
                    sessionID: "fake-content_terminal_managed_user_helper",
                    text: "whoami\n"
                ),
            "managed_user input routing must call the typed helper PTY input API"
        )
        expect(
            handle.snapshot.acceptedInputBytes == 7,
            "helper-backed managed_user handle must snapshot accepted input bytes"
        )

        let resize = handle.resize(columns: 120, rows: 40)
        expect(resize.accepted, "managed_user resize must route through helper PTY sessions")
        expect(
            helper.resizedPTYRequests.first
                == AlanManagedUserPTYResizeRequest(
                    sessionID: "fake-content_terminal_managed_user_helper",
                    columns: 120,
                    rows: 40
                ),
            "managed_user resize routing must call the typed helper PTY resize API"
        )
        expect(
            handle.snapshot.dimensions == AlanTerminalPtyDimensions(columns: 120, rows: 40),
            "helper-backed managed_user handle must snapshot helper resize dimensions"
        )

        let interrupt = handle.sendSignal(.interrupt)
        expect(interrupt.accepted, "managed_user interrupt must route through helper PTY sessions")
        expect(
            helper.signaledPTYRequests.first
                == AlanManagedUserPTYSignalRequest(
                    sessionID: "fake-content_terminal_managed_user_helper",
                    signal: .interrupt
                ),
            "managed_user signal routing must call the typed helper PTY signal API"
        )
        expect(handle.snapshot.lastSignal == .interrupt, "helper-backed handle must snapshot last signal")

        let eof = handle.closeInput()
        expect(eof.accepted, "managed_user EOF must route through helper PTY sessions")
        expect(
            helper.closedPTYInputSessionIDs == ["fake-content_terminal_managed_user_helper"],
            "managed_user EOF routing must call the typed helper PTY close-input API"
        )
        expect(handle.snapshot.inputClosed, "helper-backed handle must snapshot EOF")

        let kill = handle.sendSignal(.kill)
        expect(kill.accepted, "managed_user kill must route through helper PTY sessions")
        expect(
            helper.signaledPTYRequests.last
                == AlanManagedUserPTYSignalRequest(
                    sessionID: "fake-content_terminal_managed_user_helper",
                    signal: .kill
                ),
            "managed_user kill routing must remain helper-owned"
        )

        helper.exitObservationsBySessionID["fake-content_terminal_managed_user_helper"] =
            AlanManagedUserPTYExitObservation(
                sessionID: "fake-content_terminal_managed_user_helper",
                final: true,
                exitCode: nil,
                terminatingSignal: 9,
                sanitizedMessage: "Fake helper PTY session exited."
            )
        helper.outputChunksBySessionID["fake-content_terminal_managed_user_helper"] = [
            Data("first final helper output\n".utf8),
            Data("second final helper output\n".utf8),
        ]
        let readRequestsBeforeExitSnapshot = helper.readPTYRequests.count
        let exitedSnapshot = handle.snapshot
        expect(exitedSnapshot.phase == .exited, "helper-backed handle must project helper exit observation")
        expect(
            exitedSnapshot.exitStatus?.diagnosticsValue == "signal:9",
            "helper-backed handle must snapshot helper-reported exit status"
        )
        expect(
            exitedSnapshot.transcriptLines.contains("first final helper output"),
            "helper-backed handle must drain the first final PTY output chunk before projecting helper exit"
        )
        expect(
            exitedSnapshot.transcriptLines.contains("second final helper output"),
            "helper-backed handle must drain later final PTY output chunks before projecting helper exit"
        )
        expect(
            helper.readPTYRequests.count >= readRequestsBeforeExitSnapshot + 3,
            "helper-backed exit snapshots must keep reading until the helper reports an idle/final chunk"
        )

        let postExitDelivery = handle.writeInput("after exit")
        expect(!postExitDelivery.applied, "helper-backed handle must reject input after helper exit")
        expect(
            postExitDelivery.errorCode == "terminal_child_exited",
            "helper-backed handle must use the stable exited error code after helper exit"
        )
    }

    private static func verifiesWindowRuntimeDefaultPtyRuntimeWiresHelperProvider() {
        let request = AlanTerminalBootRequest(
            strategy: .terminalProfileManagedUser,
            executablePath: "",
            arguments: [],
            workingDirectory: "/Users/lab",
            environment: [
                "ALAN_SHELL_CONTENT_ID": "content_terminal_default_helper",
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
            ],
            bootCommand: "managed_user 'lab'",
            rendererCompatibilityCommand: nil,
            managedUserAccountName: "lab",
            terminalProfile: nil
        )
        let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
        let runtime = AlanWindowTerminalRuntimeService.makeDefaultPtyRuntime(helperClient: helper)
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_default_helper",
            bootRequest: request
        )

        expect(
            handle is AlanHelperManagedUserPtyHandle,
            "window runtime defaults must wire managed_user launches to the helper-backed PTY provider"
        )
        expect(
            helper.startedPTYRequests.first?.accountName == "lab",
            "window runtime default PTY runtime must issue helper startManagedUserPTY requests"
        )
    }

    private static func verifiesManagedUserSurfaceRoutesHelperPtyLifecycleControls() {
        let contentID = "content_terminal_managed_user_surface"
        let command = AlanCommandResolution(
            strategy: .terminalProfileManagedUser,
            executablePath: nil,
            launchPath: "",
            arguments: [],
            bootCommand: "managed_user 'lab'",
            surfaceCommand: nil,
            summary: "Managed user lab",
            detail: nil,
            repoRoot: nil,
            candidates: [],
            managedUserAccountName: "lab"
        )
        let profile = sampleBootProfile(
            workingDirectory: "/Users/lab",
            command: command,
            environment: [
                "ALAN_SHELL_CONTENT_ID": contentID,
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
            ]
        )
        let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
        let ptyRuntime = AlanDarwinTerminalPtyRuntime(
            managedUserPtyProvider: AlanHelperManagedUserPtyProvider(
                helperClient: helper,
                defaultDimensions: AlanTerminalPtyDimensions(columns: 90, rows: 25)
            )
        )
        let service = AlanWindowTerminalRuntimeService(
            bootstrap: FakeAlanGhosttyProcessBootstrap(),
            ptyRuntime: ptyRuntime
        )
        let surface = service.surfaceHandle(
            forTerminalContentID: contentID,
            mountedAtPaneID: "pane_managed_user_surface",
            bootProfile: profile
        )

        let delivery = surface.sendControlText("pwd\n")
        expect(delivery.applied, "managed_user surface input must deliver through the helper PTY handle")
        expect(
            helper.writtenPTYInputRequests.map(\.text) == ["pwd\n"],
            "managed_user surface input must call helper writeManagedUserPTY"
        )
        expect(
            ptyRuntime.registeredContentIDs.contains(contentID),
            "managed_user surface creation must register the backing PTY handle"
        )

        surface.updateHostRuntimeSnapshot(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: contentID,
                paneID: "pane_managed_user_surface",
                tabID: "tab_managed_user_surface",
                renderPriority: .foregroundInteractive,
                logicalSize: CGSize(width: 111, height: 33),
                backingSize: CGSize(width: 111, height: 33),
                displayName: nil,
                displayID: nil,
                attachedWindowTitle: nil,
                isFocused: true,
                renderer: .placeholder,
                paneMetadata: .placeholder,
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: 150)
            )
        )
        expect(
            helper.resizedPTYRequests.isEmpty,
            "managed_user surface resize must not treat logical view points as terminal rows and columns"
        )
        surface.updateHostRuntimeSnapshot(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: contentID,
                paneID: "pane_managed_user_surface",
                tabID: "tab_managed_user_surface",
                renderPriority: .foregroundInteractive,
                logicalSize: CGSize(width: 111, height: 33),
                backingSize: CGSize(width: 222, height: 66),
                displayName: nil,
                displayID: nil,
                attachedWindowTitle: nil,
                isFocused: true,
                renderer: .placeholder,
                paneMetadata: .placeholder,
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: 151)
            )
        )
        expect(
            helper.resizedPTYRequests.isEmpty,
            "managed_user surface resize must ignore host point-size-only changes without renderer grid"
        )
        surface.updateHostRuntimeSnapshot(
            TerminalHostRuntimeSnapshot(
                stage: .windowAttached,
                contentID: contentID,
                paneID: "pane_managed_user_surface",
                tabID: "tab_managed_user_surface",
                renderPriority: .foregroundInteractive,
                logicalSize: CGSize(width: 112, height: 33),
                backingSize: CGSize(width: 224, height: 66),
                displayName: nil,
                displayID: nil,
                attachedWindowTitle: nil,
                isFocused: true,
                renderer: .placeholder,
                paneMetadata: .placeholder,
                surfaceState: .placeholder,
                lastUpdatedAt: Date(timeIntervalSince1970: 152)
            )
        )
        expect(
            helper.resizedPTYRequests.isEmpty,
            "repeated point-size-only host frame changes must still not resize PTY"
        )

        let eof = surface.sendControlKey(.endOfTransmission)
        expect(eof.applied, "managed_user surface EOF must route through helper close-input")
        expect(
            helper.closedPTYInputSessionIDs == ["fake-\(contentID)"],
            "managed_user surface EOF must call helper closeManagedUserPTYInput"
        )

        let shutdown = surface.requestGracefulShutdown(reason: .paneClose)
        expect(shutdown.wasRequested, "managed_user graceful shutdown must use helper signal routing")
        expect(
            helper.signaledPTYRequests.last
                == AlanManagedUserPTYSignalRequest(sessionID: "fake-\(contentID)", signal: .interrupt),
            "managed_user graceful shutdown must call helper signalManagedUserPTY"
        )

        expect(
            service.finalizeTerminalContent(contentID) == .completed,
            "finalizing managed_user content must complete surface teardown"
        )
        expect(
            !ptyRuntime.registeredContentIDs.contains(contentID),
            "finalizing managed_user content must unregister the backing PTY handle"
        )
        expect(
            helper.terminatedPTYSessionIDs == ["fake-\(contentID)"],
            "managed_user content finalization must call helper terminatePTY exactly once"
        )
        expect(
            service.finalizeTerminalContent(contentID) == .notStarted,
            "finalized managed_user content must be evicted from the service registry"
        )
        expect(
            helper.terminatedPTYSessionIDs == ["fake-\(contentID)"],
            "managed_user content finalization must not double-terminate helper sessions"
        )
    }

    private static func verifiesManagedUserRendererAttachmentBridgesHelperSession() {
        let request = AlanTerminalBootRequest(
            strategy: .terminalProfileManagedUser,
            executablePath: "",
            arguments: [],
            workingDirectory: "/Users/lab",
            environment: [
                "ALAN_SHELL_CONTENT_ID": "content_terminal_managed_user_renderer",
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
            ],
            bootCommand: "managed_user 'lab'",
            rendererCompatibilityCommand: nil,
            managedUserAccountName: "lab",
            terminalProfile: nil
        )
        let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
        let runtime = AlanDarwinTerminalPtyRuntime(
            managedUserPtyProvider: AlanHelperManagedUserPtyProvider(helperClient: helper)
        )
        let handle = runtime.handle(
            forTerminalContentID: "content_terminal_managed_user_renderer",
            bootRequest: request
        )
        var observedShellActivity: [AlanTerminalPtyShellActivityState] = []
        handle.onShellActivityStateChange = { observedShellActivity.append($0) }
        helper.outputChunksBySessionID["fake-content_terminal_managed_user_renderer"] = [
            Data("\u{1B}]133;C\u{7}helper-output-a\n".utf8),
            Data("helper-output-b\n\u{1B}]133;D;0\u{7}\u{1B}]133;A\u{7}".utf8),
        ]

        var rendererFileDescriptor: Int32?
        defer {
            if let rendererFileDescriptor {
                close(rendererFileDescriptor)
            }
        }
        switch handle.makeRendererAttachment() {
        case .attached(let attachment):
            rendererFileDescriptor = attachment.readFileDescriptor
            expect(
                attachment.readFileDescriptor == attachment.writeFileDescriptor,
                "managed_user renderer attachment should expose one full-duplex proxy descriptor"
            )
            expect(
                attachment.closeFileDescriptors,
                "managed_user renderer attachment should let Ghostty close proxy descriptors"
            )
        case .rejected(let rejection):
            fail("managed_user renderer attachment must attach through the helper PTY proxy: \(rejection.code)")
        }
        expect(
            helper.terminatedPTYSessionIDs.isEmpty,
            "managed_user renderer attachment must not terminate a healthy helper PTY session"
        )
        guard let rendererFileDescriptor else {
            fail("managed_user renderer attachment must expose a renderer file descriptor")
        }
        let transcriptObserved = waitForPtyOutput(handle, contains: "helper-output-b")
        let helperReadDeadline = Date().addingTimeInterval(1)
        while Date() < helperReadDeadline
            && helper.readPTYRequests.filter({
                $0 == AlanManagedUserPTYReadRequest(
                    sessionID: "fake-content_terminal_managed_user_renderer",
                    maxBytes: 4096
                )
            }).count < 3
        {
            RunLoop.current.run(until: Date().addingTimeInterval(0.02))
        }
        let snapshot = handle.snapshot
        let helperReadCount = helper.readPTYRequests.filter {
            $0 == AlanManagedUserPTYReadRequest(
                sessionID: "fake-content_terminal_managed_user_renderer",
                maxBytes: 4096
            )
        }.count
        expect(
            helperReadCount >= 3,
            "managed_user renderer attachment must drain helper PTY output until idle"
        )
        expect(
            snapshot.phase == .running,
            "managed_user renderer attachment must keep the helper lifecycle running"
        )
        expect(
            transcriptObserved,
            "managed_user renderer attachment must update the fallback transcript from helper output"
        )
        expect(
            observedShellActivity == [.foregroundCommand, .shellInput],
            "managed_user renderer output must publish OSC 133 activity through the PTY handle; "
                + "observed \(observedShellActivity)"
        )

        let binaryRendererInput = Data([0xff, 0x00, 0x1b, 0x7f])
        let writtenBytes = binaryRendererInput.withUnsafeBytes { rawBuffer -> Int in
            guard let baseAddress = rawBuffer.baseAddress else { return -1 }
            return Darwin.write(rendererFileDescriptor, baseAddress, rawBuffer.count)
        }
        expect(
            writtenBytes == binaryRendererInput.count,
            "managed_user renderer attachment test must write binary renderer input"
        )
        let inputDeadline = Date().addingTimeInterval(1)
        while Date() < inputDeadline
            && !helper.writtenPTYInputRequests.contains(where: { $0.data == binaryRendererInput })
        {
            RunLoop.current.run(until: Date().addingTimeInterval(0.02))
        }
        expect(
            helper.writtenPTYInputRequests.contains(where: { $0.data == binaryRendererInput }),
            "managed_user renderer input must preserve raw bytes when writing through the helper"
        )

        helper.exitObservationsBySessionID["fake-content_terminal_managed_user_renderer"] =
            AlanManagedUserPTYExitObservation(
                sessionID: "fake-content_terminal_managed_user_renderer",
                final: true,
                exitCode: 0,
                terminatingSignal: nil,
                sanitizedMessage: "Fake helper PTY session exited."
            )
        let exitedSnapshot = handle.snapshot
        expect(
            exitedSnapshot.exitStatus?.diagnosticsValue == "exit:0",
            "managed_user renderer attachment must preserve helper-reported exit status"
        )
        helper.deniedOperation = .readManagedUserPTY
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        let stableExitSnapshot = handle.snapshot
        expect(
            stableExitSnapshot.exitStatus?.diagnosticsValue == "exit:0"
                && stableExitSnapshot.phase == .exited,
            "managed_user renderer read failures after exit must not replace exited state with failure"
        )
    }

    private static func verifiesManagedUserDirectDrainReportsShellActivity() {
        let contentID = "content_terminal_managed_user_direct_drain"
        let request = AlanTerminalBootRequest(
            strategy: .terminalProfileManagedUser,
            executablePath: "",
            arguments: [],
            workingDirectory: "/Users/lab",
            environment: [
                "ALAN_SHELL_CONTENT_ID": contentID,
                "ALAN_MANAGED_USER_ACCOUNT": "lab",
            ],
            bootCommand: "managed_user 'lab'",
            rendererCompatibilityCommand: nil,
            managedUserAccountName: "lab",
            terminalProfile: nil
        )
        let helper = AlanPrivilegedHelperFakeClient(channel: .dev)
        let runtime = AlanDarwinTerminalPtyRuntime(
            managedUserPtyProvider: AlanHelperManagedUserPtyProvider(helperClient: helper)
        )
        let handle = runtime.handle(
            forTerminalContentID: contentID,
            bootRequest: request
        )
        var observedShellActivity: [AlanTerminalPtyShellActivityState] = []
        handle.onShellActivityStateChange = { observedShellActivity.append($0) }
        let sessionID = "fake-\(contentID)"
        helper.outputChunksBySessionID[sessionID] = [
            Data("\u{1B}]133;".utf8),
            Data("C\u{7}running\n".utf8),
        ]

        _ = handle.snapshot
        expect(
            handle.shellActivityState == .foregroundCommand
                && observedShellActivity == [.foregroundCommand],
            "managed_user direct snapshot drains must publish split OSC 133 command-start activity; "
                + "observed \(observedShellActivity)"
        )

        helper.outputChunksBySessionID[sessionID] = [
            Data("\u{1B}]133;D;0\u{7}\u{1B}]133;A\u{7}".utf8)
        ]
        _ = handle.snapshot
        expect(
            handle.shellActivityState == .shellInput
                && observedShellActivity == [.foregroundCommand, .shellInput],
            "managed_user direct snapshot drains must publish prompt activity; "
                + "observed \(observedShellActivity)"
        )

        helper.outputChunksBySessionID[sessionID] = [
            Data("\u{1B}]133;".utf8)
        ]
        _ = handle.snapshot
        expect(
            handle.shellActivityState == .shellInput,
            "an incomplete direct-drain OSC marker must not publish activity"
        )

        helper.outputChunksBySessionID[sessionID] = [
            Data("C\u{7}renderer-running\n".utf8)
        ]
        let rendererFileDescriptor: Int32
        switch handle.makeRendererAttachment() {
        case .attached(let attachment):
            rendererFileDescriptor = attachment.readFileDescriptor
        case .rejected(let rejection):
            fail("managed_user parser handoff requires a renderer attachment: \(rejection.code)")
        }
        let rendererDeadline = Date().addingTimeInterval(1)
        while Date() < rendererDeadline
            && handle.shellActivityState != .foregroundCommand
        {
            _ = handle.snapshot
            RunLoop.current.run(until: Date().addingTimeInterval(0.02))
        }
        expect(
            handle.shellActivityState == .foregroundCommand
                && observedShellActivity
                    == [.foregroundCommand, .shellInput, .foregroundCommand],
            "managed_user renderer attachment must continue the direct drain parser state; "
                + "observed \(observedShellActivity)"
        )

        close(rendererFileDescriptor)
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        helper.outputChunksBySessionID[sessionID] = [
            Data("\u{1B}]133;D;0\u{7}\u{1B}]133;A\u{7}".utf8)
        ]
        let detachedDeadline = Date().addingTimeInterval(1)
        while Date() < detachedDeadline
            && handle.shellActivityState != .shellInput
        {
            _ = handle.snapshot
            RunLoop.current.run(until: Date().addingTimeInterval(0.02))
        }
        expect(
            handle.shellActivityState == .shellInput
                && observedShellActivity
                    == [.foregroundCommand, .shellInput, .foregroundCommand, .shellInput],
            "managed_user direct drains must resume after renderer detachment; "
                + "observed \(observedShellActivity)"
        )
    }

    private static func verifiesAlanGhosttySurfaceDeliveryUsesPtyRuntimeWithoutRenderer() {
        let contentID = "content_terminal_surface_pty"
        let profile = sampleBootProfile(
            workingDirectory: "/tmp/project",
            environment: ["ALAN_SHELL_CONTENT_ID": contentID]
        )
        let runtime = FakeAlanTerminalPtyRuntime()
        let surface = AlanGhosttySurfaceHandle(
            contentID: contentID,
            paneID: "pane_surface_pty",
            bootstrap: FakeAlanGhosttyProcessBootstrap(),
            ptyRuntime: runtime
        )
        surface.configure(mountedAtPaneID: "pane_surface_pty", bootProfile: profile)

        expect(!surface.isSurfaceReady, "renderer must not be required for PTY delivery readiness")
        expect(
            surface.snapshot.metadata.activeTaskState == .unknown,
            "unknown PTY shell activity must fail closed before any input is delivered"
        )

        let delivery = surface.sendControlText("pwd\n")
        let handle = runtime.existingHandle(forTerminalContentID: contentID)
            as! FakeAlanTerminalPtyHandle
        expect(delivery.applied, "surface delivery must be accepted by Alan-owned PTY runtime")
        expect(
            handle.deliveredText == ["pwd\n"],
            "surface delivery must write to the PTY handle rather than Ghostty renderer text"
        )
        expect(
            handle.shellActivityState == .unknown,
            "accepted newline text must not infer PTY shell activity"
        )
        expect(
            surface.snapshot.metadata.activeTaskState == .unknown,
            "unknown PTY shell activity must fail closed until a prompt marker is observed"
        )

        handle.recordShellActivityState(.foregroundCommand)
        expect(
            surface.snapshot.metadata.activeTaskState == .foregroundCommand,
            "Alan-owned PTY shell activity must protect the terminal surface"
        )
        handle.recordShellActivityState(.shellInput)
        expect(
            surface.snapshot.metadata.activeTaskState == .inactive,
            "returning to shell input must clear foreground command protection"
        )

        let shutdown = surface.requestGracefulShutdown(reason: .paneClose)
        expect(shutdown.wasRequested, "graceful shutdown must use Alan-owned signal delivery")
        expect(
            handle.signalRequests == [.interrupt],
            "graceful shutdown must signal the Alan-owned process handle"
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

        let foregroundPrevious = sampleRuntimeSnapshot(
            priority: .foregroundInteractive,
            metadata: .placeholder,
            lastUpdatedAt: Date(timeIntervalSince1970: 6),
            surfaceState: AlanTerminalSurfaceStateSnapshot(
                readiness: .ready,
                terminalMode: .normalBuffer,
                scrollback: AlanTerminalScrollbackState(
                    metrics: AlanTerminalScrollbackMetrics(
                        totalRows: 200,
                        visibleRows: 40,
                        firstVisibleRow: 160,
                        mode: .normalBuffer
                    ),
                    nativeScrollbarVisible: true,
                    thumbRange: 160..<200
                ),
                search: nil,
                semanticCommands: .placeholder,
                readonly: false,
                secureInput: false,
                inputReady: true,
                rendererHealth: "ready",
                childExited: false,
                lastUpdatedAt: Date(timeIntervalSince1970: 6)
            )
        )
        let foreground = sampleRuntimeSnapshot(
            priority: .foregroundInteractive,
            metadata: .placeholder,
            lastUpdatedAt: Date(timeIntervalSince1970: 7),
            surfaceState: AlanTerminalSurfaceStateSnapshot(
                readiness: .ready,
                terminalMode: .normalBuffer,
                scrollback: AlanTerminalScrollbackState(
                    metrics: AlanTerminalScrollbackMetrics(
                        totalRows: 240,
                        visibleRows: 40,
                        firstVisibleRow: 200,
                        mode: .normalBuffer
                    ),
                    nativeScrollbarVisible: true,
                    thumbRange: 200..<240
                ),
                search: nil,
                semanticCommands: .placeholder,
                readonly: false,
                secureInput: false,
                inputReady: true,
                rendererHealth: "ready",
                childExited: false,
                lastUpdatedAt: Date(timeIntervalSince1970: 7)
            )
        )
        expect(
            !TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: foregroundPrevious,
                next: foreground
            ),
            "foreground scrollback churn must stay inside the terminal runtime"
        )

        let foregroundTitleChange = sampleRuntimeSnapshot(
            priority: .foregroundInteractive,
            metadata: TerminalPaneMetadataSnapshot(
                title: "cargo test",
                workingDirectory: nil,
                summary: nil,
                attention: .idle,
                processExited: false,
                lastCommandExitCode: nil,
                lastUpdatedAt: Date(timeIntervalSince1970: 7)
            ),
            lastUpdatedAt: Date(timeIntervalSince1970: 7),
            surfaceState: foregroundPrevious.surfaceState
        )
        expect(
            TerminalRuntimePublicationPolicy.shouldProjectToShell(
                previous: foregroundPrevious,
                next: foregroundTitleChange
            ),
            "foreground title changes must remain publishable for sidebar summaries"
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
        handle.terminalDimensionsOverride = AlanTerminalPtyDimensions(columns: 100, rows: 30)
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
        expect(snapshot.dimensions?.columns == 100, "capture must preserve PTY terminal columns")
        expect(snapshot.dimensions?.rows == 30, "capture must preserve PTY terminal rows")
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

    private static func waitForDarwinPtyOutput(
        _ handle: AlanDarwinTerminalPtyHandle,
        contains needle: String,
        timeout: TimeInterval = 2
    ) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            _ = handle.drainAvailableOutput()
            let transcript = handle.snapshot.transcriptLines.joined(separator: "\n")
            if transcript.contains(needle) {
                return true
            }
            usleep(50_000)
        }
        return false
    }

    private static func waitForPtyOutput(
        _ handle: AlanTerminalPtyHandle,
        contains needle: String,
        timeout: TimeInterval = 2
    ) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            let transcript = handle.snapshot.transcriptLines.joined(separator: "\n")
            if transcript.contains(needle) {
                return true
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        return handle.snapshot.transcriptLines.joined(separator: "\n").contains(needle)
    }

    private static func waitForDarwinPtyExit(
        _ handle: AlanDarwinTerminalPtyHandle,
        timeout: TimeInterval = 2
    ) -> AlanTerminalProcessExitStatus? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let status = handle.refreshExitStatus() {
                _ = handle.drainAvailableOutput()
                return status
            }
            usleep(50_000)
        }
        return handle.refreshExitStatus()
    }

    private static func sampleRuntimeSnapshot(
        priority: TerminalRuntimeRenderPriority,
        metadata: TerminalPaneMetadataSnapshot,
        renderer: TerminalRendererSnapshot = .placeholder,
        lastUpdatedAt: Date,
        surfaceState: AlanTerminalSurfaceStateSnapshot = .placeholder
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
            surfaceState: surfaceState,
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
