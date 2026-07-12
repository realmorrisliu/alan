import Foundation

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    guard condition() else {
        fputs("terminal-account-dev-dry-run-smoke: \(message)\n", stderr)
        exit(1)
    }
}
@main
private enum TerminalAccountDevDryRunSmoke {
    static func main() throws {
        let fileManager = FileManager.default
        let root = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("alan-terminal-account-dev-dry-run-\(UUID().uuidString)", isDirectory: true)
        let appSupport = root.appendingPathComponent("app-support", isDirectory: true)
        let devProfileStore = appSupport
            .appendingPathComponent("alan-macos-dev", isDirectory: true)
            .appendingPathComponent("terminal-profiles.json", isDirectory: false)
        let stableProfileStore = appSupport
            .appendingPathComponent("alan-macos", isDirectory: true)
            .appendingPathComponent("terminal-profiles.json", isDirectory: false)

        try fileManager.createDirectory(at: appSupport, withIntermediateDirectories: true)

        let request = ManagedTerminalAccountRequest(
            accountName: "alan_smoke",
            fullName: "Alan Smoke",
            shell: "/bin/zsh",
            homeDirectory: "/Users/alan_smoke",
            hideFromLoginWindow: true
        )
        let missingDiagnosis = AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .missing,
            readinessState: .accountMissing,
            accountExists: false,
            isAdmin: false,
            homeDirectoryExists: false,
            shellMatches: false,
            hiddenFromLoginWindow: false,
            terminalProfileID: nil,
            ptySmokeVerified: false,
            diagnostic: nil
        )
        let plan = ManagedTerminalAccountPlanner.plan(request: request, diagnosis: missingDiagnosis)
        let planKinds = plan.steps.map(\.kind)

        expect(plan.status == .readyToApply, "missing account dry run must be ready to apply")
        expect(
            planKinds.contains(.helperStep(.createStandardAccount)),
            "dry run must include helper-backed account creation"
        )
        expect(
            planKinds.contains(.helperStep(.hideAccount)),
            "dry run must include helper-backed login-window hiding"
        )
        expect(
            planKinds.contains(.helperStep(.writeOwnershipMarker)),
            "dry run must include Alan-managed ownership marker"
        )
        expect(
            planKinds.contains(.helperStep(.verifyManagedUserPTY)),
            "dry run must include helper PTY readiness verification"
        )
        expect(planKinds.contains(.createOrUpdateTerminalProfile), "dry run must include profile handoff")

        let cancelledExecutor = ManagedTerminalAccountFakeExecutor()
        cancelledExecutor.cancelBeforeApply = true
        let cancelled = cancelledExecutor.apply(plan)
        expect(cancelled.cancelled, "cancelled preview must not apply privileged changes")
        expect(cancelled.completedSteps.isEmpty, "cancelled preview must not complete steps")
        expect(
            !fileManager.fileExists(atPath: devProfileStore.path),
            "cancelled dry run must not create a dev profile store"
        )
        expect(
            !fileManager.fileExists(atPath: stableProfileStore.path),
            "cancelled dry run must not create a stable profile store"
        )

        let readyState = ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/alan_smoke", shell: "/bin/zsh", hidden: true),
            ownership: .alanManaged(.helperMarker(path: "/Library/Application Support/alan-macos-dev/privileged-helper/managed-users/alan_smoke/ownership.json")),
            terminalProfile: .missing,
            verification: .passed
        )
        guard let handoff = ManagedTerminalAccountProfileHandoff.profileDefinition(
            for: request,
            state: readyState
        ) else {
            fputs("terminal-account-dev-dry-run-smoke: ready state did not produce handoff profile\n", stderr)
            exit(1)
        }

        let store = TerminalProfileStore.defaultStore(
            channelApplicationSupportDirectoryName: "alan-macos-dev",
            fileManager: fileManager,
            environment: [
                "ALAN_INSTALL_CHANNEL": "dev",
                "ALAN_MACOS_APPLICATION_SUPPORT_DIR": appSupport.path,
            ]
        )
        try store.save(
            TerminalProfileDocument(
                defaultProfileID: TerminalProfileDefinition.loginShellFallback.id,
                profiles: [TerminalProfileDefinition.loginShellFallback, handoff]
            )
        )
        let loaded = store.load().document.profile(id: "alan_smoke")

        expect(fileManager.fileExists(atPath: devProfileStore.path), "handoff must write dev profile store")
        expect(
            !fileManager.fileExists(atPath: stableProfileStore.path),
            "dev-channel handoff must not create stable profile store"
        )
        expect(loaded?.managedTerminalAccountID == "alan_smoke", "profile must link managed account")
        expect(loaded?.launch == .managedUser(unixUser: "alan_smoke"), "profile must use managed_user launch")
        expect(
            store.load().document.defaultProfileID == TerminalProfileDefinition.loginShellFallback.id,
            "handoff must not change the default terminal identity from Login shell"
        )

        print("terminal account dev dry-run smoke passed")
        print("tmp_root=\(root.path)")
        print("dev_profile_store=\(devProfileStore.path)")
    }
}
