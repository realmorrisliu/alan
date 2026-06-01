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
            guiUserName: "morris",
            fullName: "Alan Smoke",
            shell: "/bin/zsh",
            homeDirectory: "/Users/alan_smoke",
            hideFromLoginWindow: true,
            bindCurrentSpaceAfterSuccess: true
        )
        let missingState = ManagedTerminalAccountState(
            account: .missing,
            sudoers: .missing,
            terminalProfile: .missing,
            verification: .notRun
        )
        let plan = ManagedTerminalAccountPlanner.plan(request: request, state: missingState)
        let planKinds = plan.steps.map(\.kind)

        expect(plan.status == .readyToApply, "missing account dry run must be ready to apply")
        expect(planKinds.contains(.createStandardAccount), "dry run must include account creation")
        expect(planKinds.contains(.hideAccount), "dry run must include login-window hiding")
        expect(planKinds.contains(.writeSudoersDropIn), "dry run must include sudoers write")
        expect(planKinds.contains(.validateSudoers), "dry run must include sudoers validation")
        expect(planKinds.contains(.verifyTerminalEntry), "dry run must include terminal entry verification")
        expect(planKinds.contains(.createOrUpdateTerminalProfile), "dry run must include profile handoff")
        expect(planKinds.contains(.bindCurrentSpace), "dry run must include explicit Space binding step")

        let rule = ManagedTerminalAccountSudoersRule(request: request)
        expect(
            rule.filePath == "/etc/sudoers.d/alan-terminal-morris-to-alan_smoke",
            "sudoers path must be deterministic and Alan-owned"
        )
        expect(
            rule.contents.contains("morris ALL=(alan_smoke) NOPASSWD: ALL"),
            "sudoers rule must target only the managed account"
        )
        expect(!rule.contents.contains("ALL=(ALL)"), "sudoers rule must not grant passwordless root")
        expect(
            !rule.contents.contains("morris ALL=(root)"),
            "sudoers rule must not grant direct root entry"
        )

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
            sudoers: .alanOwnedValid(path: rule.filePath),
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
        try store.save(TerminalProfileDocument(defaultProfileID: handoff.id, profiles: [handoff]))
        let loaded = store.load().document.profile(id: "alan_smoke")

        expect(fileManager.fileExists(atPath: devProfileStore.path), "handoff must write dev profile store")
        expect(
            !fileManager.fileExists(atPath: stableProfileStore.path),
            "dev-channel handoff must not create stable profile store"
        )
        expect(loaded?.managedTerminalAccountID == "alan_smoke", "profile must link managed account")
        expect(loaded?.launch == .sudoUser(unixUser: "alan_smoke"), "profile must use sudo_user launch")

        print("terminal account dev dry-run smoke passed")
        print("tmp_root=\(root.path)")
        print("dev_profile_store=\(devProfileStore.path)")
    }
}
