import Foundation
import OSLog
import SwiftUI

struct ShellSettingsContentView: View {
    let descriptor: ShellContentRenderDescriptor

    @AppStorage("alanShellAppearanceMode") private var appearanceMode = ShellAppearanceMode.system
    @AppStorage("alanShellSidebarCollapsed") private var isSidebarCollapsed = false
    @AppStorage("alanShellDimsInactiveSplitPanes") private var dimsInactiveSplitPanes = true
    @AppStorage(AlanPerformanceDiagnosticsController.preferenceKey)
    private var performanceDiagnosticsEnabled = false
    @State private var localSummary = ShellSettingsLocalSummary.current()
    @State private var terminalProfilesSummary = TerminalProfileSettingsSummary.current()
    @State private var privilegedHelperSummary = PrivilegedHelperSettingsSummary.current()
    @State private var managedTerminalAccountsSummary = ManagedTerminalAccountSettingsSummary.empty
    @State private var lastDiagnosticsExportURL: URL?
    @State private var selectedGroup = ShellSettingsNavigationGroup.general
    @State private var isManagedUserCreationPresented = false
    @State private var managedUserCreationDraft = ManagedTerminalUserCreationDraft(
        unixUserName: "",
        displayLabel: ""
    )
    @State private var managedUserCreationPreviewResult: ManagedTerminalUserCreationPreviewResult?
    @State private var managedUserActionSheet: ShellManagedUserActionSheetState?
    @State private var managedUserApplyDiagnostics: [String] = []
    @State private var managedUserApplyInFlight = false

    nonisolated private static let managedUserApplyLogger = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "app.alanworks.macos",
        category: "ManagedUsers"
    )
    nonisolated private static let managedUserApplyTimeoutNanoseconds: UInt64 =
        10 * 60 * 1_000_000_000

    private var snapshot: ShellSettingsSurfaceSnapshot {
        ShellSettingsSurfaceSnapshot.make(
            local: localSummary,
            terminalProfiles: terminalProfilesSummary,
            privilegedHelper: privilegedHelperSummary,
            managedTerminalAccounts: managedTerminalAccountsSummary,
            diagnostics: diagnosticsSummary
        )
    }

    private var settingsGroups: [ShellSettingsNavigationGroupModel] {
        snapshot.navigationGroups
    }

    private var selectedGroupModel: ShellSettingsNavigationGroupModel {
        settingsGroups.first { $0.id == selectedGroup }
            ?? settingsGroups.first
            ?? ShellSettingsNavigationGroupModel(id: .general, sections: [])
    }

    var body: some View {
        ZStack {
            ShellSettingsBackdrop()

            HStack(alignment: .top, spacing: 0) {
                ShellSettingsNavigationView(
                    groups: settingsGroups,
                    selectedGroup: $selectedGroup
                )
                .frame(width: ShellSettingsMetrics.navigationWidth, alignment: .topLeading)
                .padding(.leading, ShellSettingsMetrics.navigationLeadingPadding)
                .padding(.trailing, ShellSettingsMetrics.navigationTrailingPadding)
                .padding(.top, ShellSettingsMetrics.navigationTopPadding)
                .frame(maxHeight: .infinity, alignment: .topLeading)
                .background {
                    ShellSettingsNavigationRailBackground()
                }

                ZStack(alignment: .topLeading) {
                    ShellSettingsDetailBackground()

                    ScrollView {
                        ShellSettingsGroupView(
                            group: selectedGroupModel,
                            appearanceMode: $appearanceMode,
                            sidebarVisible: sidebarVisible,
                            dimsInactiveSplitPanes: $dimsInactiveSplitPanes,
                            performanceDiagnosticsEnabled: performanceDiagnosticsBinding,
                            onExportPerformanceDiagnostics: exportPerformanceDiagnostics,
                            onRowAction: handleSettingsRowAction
                        )
                        .frame(maxWidth: ShellSettingsMetrics.contentWidth, alignment: .leading)
                        .padding(.leading, ShellSettingsMetrics.detailContentLeadingPadding)
                        .padding(.trailing, ShellSettingsMetrics.detailContentTrailingPadding)
                        .padding(.top, ShellSettingsMetrics.detailContentTopPadding)
                        .padding(.bottom, ShellSettingsMetrics.detailContentBottomPadding)
                        .frame(maxWidth: .infinity, alignment: .topLeading)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .task(id: refreshTaskID) {
            await refreshSettingsSummaries()
        }
        .sheet(isPresented: $isManagedUserCreationPresented) {
            ShellManagedUserCreationSheet(
                draft: $managedUserCreationDraft,
                previewResult: managedUserCreationPreviewResult,
                diagnostics: managedUserApplyDiagnostics,
                isApplying: managedUserApplyInFlight,
                onDraftChanged: resetManagedUserCreationPreview,
                onPreview: reviewManagedUserCreationDraft,
                onApply: applyManagedUserCreationPreview,
                onCancel: {
                    isManagedUserCreationPresented = false
                }
            )
        }
        .sheet(item: $managedUserActionSheet) { sheet in
            ShellManagedUserPlanSheet(
                sheet: sheet,
                diagnostics: managedUserApplyDiagnostics,
                isApplying: managedUserApplyInFlight,
                onApply: {
                    applyManagedUserActionSheet(sheet)
                },
                onCancel: {
                    managedUserActionSheet = nil
                }
            )
        }
    }

    private var refreshTaskID: String {
        descriptor.contentID ?? descriptor.title
    }

    private var sidebarVisible: Binding<Bool> {
        Binding(
            get: { !isSidebarCollapsed },
            set: { isSidebarCollapsed = !$0 }
        )
    }

    private var performanceDiagnosticsBinding: Binding<Bool> {
        Binding(
            get: { performanceDiagnosticsEnabled },
            set: { nextValue in
                performanceDiagnosticsEnabled = nextValue
                AlanPerformanceDiagnosticsController.shared.setEnabled(nextValue)
            }
        )
    }

    private var diagnosticsSummary: ShellSettingsDiagnosticsSummary {
        let summary = AlanPerformanceDiagnosticsController.shared.summarySnapshot()
        return ShellSettingsDiagnosticsSummary(
            isEnabled: performanceDiagnosticsEnabled,
            retainedEventCount: AlanPerformanceDiagnosticsController.shared.eventsSnapshot().count,
            stutterMarkerCount: summary.stutterMarkerCount,
            lastExportURL: lastDiagnosticsExportURL
        )
    }

    private func exportPerformanceDiagnostics() {
        lastDiagnosticsExportURL = AlanPerformanceDiagnosticsExportPresenter.exportRecentDiagnostics(
            installChannel: localSummary.channelLabel
        )
    }

    @discardableResult
    @MainActor
    private func refreshLocalTerminalIdentitySummaries() -> ManagedTerminalAccountSettingsSummary {
        let profiles = TerminalProfileSettingsSummary.current()
        let accounts = ManagedTerminalAccountSettingsSummary.current(
            terminalProfiles: profiles,
            helperClient: AlanPrivilegedHelperAppClient(channel: .current())
        )
        terminalProfilesSummary = profiles
        managedTerminalAccountsSummary = accounts
        return accounts
    }

    @MainActor
    private func handleSettingsRowAction(
        row: ShellSettingsRowModel,
        action: ShellSettingsRowActionKind
    ) {
        managedUserApplyDiagnostics = []
        switch action {
        case .create:
            managedUserCreationDraft = ManagedTerminalUserCreationDraft(
                unixUserName: "",
                displayLabel: ""
            )
            managedUserCreationPreviewResult = nil
            isManagedUserCreationPresented = true
        case .review, .repair, .verify, .remove:
            handleExistingManagedUserAction(row: row, action: action)
        case .installHelper, .updateHelper, .uninstallHelper:
            applyPrivilegedHelperLifecycleAction(action)
        }
    }

    @MainActor
    private func applyPrivilegedHelperLifecycleAction(_ action: ShellSettingsRowActionKind) {
        let manager = AlanPrivilegedHelperAppServiceManager()
        let result: AlanPrivilegedHelperLifecycleResult
        switch action {
        case .installHelper, .updateHelper:
            result = manager.installOrUpdate()
        case .uninstallHelper:
            result = manager.uninstall()
        case .create, .review, .repair, .verify, .remove:
            return
        }
        privilegedHelperSummary = PrivilegedHelperSettingsSummary(status: result.status)
        managedUserApplyDiagnostics = result.diagnostic.map {
            [$0.sanitizedMessage, "Credentials redacted."]
        } ?? ["Privileged helper \(result.action.rawValue) completed. Credentials redacted."]
    }

    @MainActor
    private func handleExistingManagedUserAction(
        row: ShellSettingsRowModel,
        action: ShellSettingsRowActionKind
    ) {
        guard let plan = managedUserPlan(forRowID: row.id) else { return }
        switch action {
        case .create, .installHelper, .updateHelper, .uninstallHelper:
            return
        case .review:
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: action,
                plan: plan,
                allowsApply: false
            )
        case .repair:
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: action,
                plan: plan,
                allowsApply: true
            )
        case .verify:
            let refreshedSummary = refreshLocalTerminalIdentitySummaries()
            let refreshedPlan = refreshedSummary.plans.first {
                $0.request.accountName == plan.request.accountName
            } ?? plan
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: .review,
                plan: refreshedPlan,
                allowsApply: false
            )
        case .remove:
            let helperClient = AlanPrivilegedHelperAppClient(channel: .current())
            let status = helperClient.status()
            let diagnosis = status.isHealthy
                ? helperClient.diagnoseManagedUser(plan.request)
                : AlanManagedUserDiagnosis.helperUnavailable(request: plan.request, status: status)
            let rollbackPlan = ManagedTerminalAccountPlanner.rollbackPlan(
                request: plan.request,
                diagnosis: diagnosis,
                scope: .alanIntegrationOnly,
                terminalProfiles: terminalProfilesSummary.document
            )
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: action,
                plan: rollbackPlan,
                allowsApply: true
            )
        }
    }

    private func managedUserPlan(forRowID rowID: String) -> ManagedTerminalAccountPlan? {
        let prefix = "terminalAccount."
        guard rowID.hasPrefix(prefix) else { return nil }
        let accountName = String(rowID.dropFirst(prefix.count))
        return managedTerminalAccountsSummary.plans.first {
            $0.request.accountName == accountName
        }
    }

    @MainActor
    private func resetManagedUserCreationPreview() {
        managedUserCreationPreviewResult = nil
        managedUserApplyDiagnostics = []
    }

    @MainActor
    private func reviewManagedUserCreationDraft() {
        let request = managedUserCreationDraft.request
        let helperClient = AlanPrivilegedHelperAppClient(channel: .current())
        let status = helperClient.status()
        let diagnosis = status.isHealthy
            ? helperClient.diagnoseManagedUser(request)
            : AlanManagedUserDiagnosis.helperUnavailable(request: request, status: status)
        managedUserCreationPreviewResult = ManagedTerminalUserCreationPreviewBuilder.make(
            draft: managedUserCreationDraft,
            existingUsers: managedTerminalAccountsSummary.users,
            terminalProfiles: terminalProfilesSummary,
            diagnosis: diagnosis
        )
    }

    @MainActor
    private func applyManagedUserCreationPreview(_ preview: ManagedTerminalUserCreationPreview) {
        guard !managedUserApplyInFlight else { return }
        managedUserApplyInFlight = true
        managedUserApplyDiagnostics = ["Applying managed user changes. Credentials redacted."]

        Task {
            defer { managedUserApplyInFlight = false }
            let result = await Self.applyManagedUserPlanInBackground(
                plan: preview.plan,
                request: preview.request
            )
            if Task.isCancelled { return }

            terminalProfilesSummary = result.terminalProfiles
            managedTerminalAccountsSummary = result.managedTerminalAccounts
            managedUserApplyDiagnostics = result.applyResult.visibleDiagnostics
            if !result.applyResult.cancelled && result.applyResult.failedStep == nil {
                isManagedUserCreationPresented = false
                managedUserCreationPreviewResult = nil
            }
        }
    }

    @MainActor
    private func applyManagedUserActionSheet(_ sheet: ShellManagedUserActionSheetState) {
        guard !managedUserApplyInFlight else { return }
        managedUserApplyInFlight = true
        managedUserApplyDiagnostics = ["Applying managed user changes. Credentials redacted."]

        Task {
            defer { managedUserApplyInFlight = false }
            let result = await Self.applyManagedUserPlanInBackground(
                plan: sheet.plan,
                request: sheet.plan.request
            )
            if Task.isCancelled { return }

            terminalProfilesSummary = result.terminalProfiles
            managedTerminalAccountsSummary = result.managedTerminalAccounts
            managedUserApplyDiagnostics = result.applyResult.visibleDiagnostics
            if !result.applyResult.cancelled && result.applyResult.failedStep == nil {
                managedUserActionSheet = nil
            }
        }
    }

    nonisolated private static func applyManagedUserPlanInBackground(
        plan: ManagedTerminalAccountPlan,
        request: ManagedTerminalAccountRequest
    ) async -> ShellManagedUserApplyBackgroundResult {
        await withCheckedContinuation { continuation in
            let continuationBox = ShellManagedUserApplyContinuationBox(continuation: continuation)
            let work = Task.detached(priority: .userInitiated) {
                let result = runManagedUserPlanInBackground(plan: plan, request: request)
                continuationBox.resume(returning: result)
            }
            Task.detached(priority: .userInitiated) {
                try? await Task.sleep(nanoseconds: managedUserApplyTimeoutNanoseconds)
                guard !work.isCancelled else { return }
                work.cancel()
                managedUserApplyLogger.error("Managed User apply timed out.")
                continuationBox.resume(returning: timeoutManagedUserApplyResult(plan: plan))
            }
        }
    }

    nonisolated private static func runManagedUserPlanInBackground(
        plan: ManagedTerminalAccountPlan,
        request: ManagedTerminalAccountRequest
    ) -> ShellManagedUserApplyBackgroundResult {
        managedUserApplyLogger.info("Managed User apply started.")
        let catalogStore = ManagedTerminalAccountCatalogStore.defaultStore()
        let isRemovalPlan = plan.steps.contains {
            switch $0.kind {
            case .removeManagedTerminalProfile, .deleteAccount, .deleteHomeDirectory:
                return true
            case .helperStep(let helperKind):
                return helperKind == .removeManagedUserIntegration
                    || helperKind == .deleteAccount
                    || helperKind == .deleteHomeDirectory
            default:
                return false
            }
        } && !plan.steps.contains {
            switch $0.kind {
            case .createStandardAccount,
                 .repairAccountType,
                 .repairHomeDirectory,
                 .repairShell,
                 .hideAccount,
                 .createOrUpdateTerminalProfile:
                return true
            case .helperStep(let helperKind):
                switch helperKind {
                case .removeManagedUserIntegration, .deleteAccount, .deleteHomeDirectory:
                    return false
                case .createStandardAccount,
                     .repairAccountType,
                     .repairHomeDirectory,
                     .repairShell,
                     .hideAccount,
                     .writeOwnershipMarker,
                     .verifyAccount,
                     .verifyManagedUserPTY:
                    return true
                }
            case .removeManagedTerminalProfile, .deleteAccount, .deleteHomeDirectory:
                return false
            }
        }
        if !isRemovalPlan {
            try? catalogStore.upsert(
                ManagedTerminalAccountCatalogEntry(
                    accountName: request.accountName,
                    displayLabel: request.fullName ?? request.accountName
                )
            )
        }
        let channel = AlanInstallChannel.current()
        let helperClient = AlanPrivilegedHelperAppClient(channel: channel)
        let executor = ManagedTerminalAccountHelperExecutor(
            channel: channel,
            helperClient: helperClient
        )
        let applyResult = executor.apply(plan)
        if isRemovalPlan && !applyResult.cancelled && applyResult.failedStep == nil {
            try? catalogStore.remove(accountName: request.accountName)
        }
        let terminalProfiles = TerminalProfileSettingsSummary.current()
        let managedTerminalAccounts = ManagedTerminalAccountSettingsSummary.current(
            terminalProfiles: terminalProfiles,
            helperClient: helperClient
        )
        managedUserApplyLogger.info("Managed User apply finished.")
        return ShellManagedUserApplyBackgroundResult(
            applyResult: applyResult,
            terminalProfiles: terminalProfiles,
            managedTerminalAccounts: managedTerminalAccounts
        )
    }

    nonisolated private static func timeoutManagedUserApplyResult(
        plan: ManagedTerminalAccountPlan
    ) -> ShellManagedUserApplyBackgroundResult {
        let terminalProfiles = TerminalProfileSettingsSummary.current()
        return ShellManagedUserApplyBackgroundResult(
            applyResult: ManagedTerminalAccountApplyResult(
                completedSteps: [],
                failedStep: plan.steps.first?.kind,
                cancelled: false,
                visibleDiagnostics: [
                    "Managed User apply timed out. Credentials redacted.",
                ]
            ),
            terminalProfiles: terminalProfiles,
            managedTerminalAccounts: ManagedTerminalAccountSettingsSummary.current(
                terminalProfiles: terminalProfiles,
                helperClient: AlanPrivilegedHelperAppClient(channel: .current())
            )
        )
    }

    private struct ShellManagedUserApplyBackgroundResult {
        let applyResult: ManagedTerminalAccountApplyResult
        let terminalProfiles: TerminalProfileSettingsSummary
        let managedTerminalAccounts: ManagedTerminalAccountSettingsSummary
    }

    private final class ShellManagedUserApplyContinuationBox: @unchecked Sendable {
        private let lock = NSLock()
        private var hasResumed = false
        private let continuation: CheckedContinuation<ShellManagedUserApplyBackgroundResult, Never>

        init(continuation: CheckedContinuation<ShellManagedUserApplyBackgroundResult, Never>) {
            self.continuation = continuation
        }

        func resume(returning result: ShellManagedUserApplyBackgroundResult) {
            lock.lock()
            defer { lock.unlock() }
            guard !hasResumed else { return }
            hasResumed = true
            continuation.resume(returning: result)
        }
    }

    @MainActor
    private func refreshSettingsSummaries() async {
        localSummary = ShellSettingsLocalSummary.current()
        privilegedHelperSummary = PrivilegedHelperSettingsSummary.current()
        refreshLocalTerminalIdentitySummaries()
    }
}
