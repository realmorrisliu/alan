import Foundation

extension ShellCoreFFIAdapter {
    func terminalProfileRows(
        _ summary: TerminalProfileSettingsSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.terminal_profile_rows",
            payload: ShellCoreTerminalProfileSettingsSummaryPayload(summary)
        )
        return response.rows.map(\.settingsRow)
    }

    func managedTerminalAccountRows(
        _ summary: ManagedTerminalAccountSettingsSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.managed_terminal_account_rows",
            payload: ShellCoreManagedTerminalAccountSettingsSummaryPayload(summary)
        )
        return response.rows.map(\.settingsRow)
    }

    func capabilityRows(
        _ summary: ShellSettingsCapabilitiesSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.capability_rows",
            payload: ShellCoreCapabilitiesSettingsSummaryPayload(summary)
        )
        return response.rows.map(\.settingsRow)
    }

    func localRows(
        _ local: ShellSettingsLocalSummary,
        diagnostics: ShellSettingsDiagnosticsSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.local_rows",
            payload: ShellCoreLocalRowsPayload(local: local, diagnostics: diagnostics)
        )
        return response.rows.map(\.settingsRow)
    }

}

private struct ShellCoreSettingsRowsResponse: Decodable {
    let rows: [ShellCoreSettingsRowSummary]
}

private struct ShellCoreSettingsRowSummary: Decodable {
    let id: String
    let systemName: String
    let title: String
    let detail: String?
    let value: String?
    let mutability: ShellCoreSettingsRowMutability
    let offersFreeformEditing: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case systemName = "system_name"
        case title
        case detail
        case value
        case mutability
        case offersFreeformEditing = "offers_freeform_editing"
    }

    var settingsRow: ShellSettingsRowModel {
        ShellSettingsRowModel(
            id: id,
            systemName: systemName,
            title: title,
            detail: detail,
            value: value,
            mutability: mutability.settingsMutability,
            offersFreeformEditing: offersFreeformEditing
        )
    }
}

private enum ShellCoreSettingsRowMutability: String, Decodable {
    case editable
    case readOnly = "read_only"
    case actionOnly = "action_only"
    case deferred

    var settingsMutability: ShellSettingsRowMutability {
        switch self {
        case .editable:
            return .editable
        case .readOnly:
            return .readOnly
        case .actionOnly:
            return .actionOnly
        case .deferred:
            return .deferred
        }
    }
}

private struct ShellCoreTerminalProfileSettingsSummaryPayload: Encodable {
    let profiles: [TerminalProfileDefinition]
    let defaultProfileID: String
    let recoveryMessage: String?

    private enum CodingKeys: String, CodingKey {
        case profiles
        case defaultProfileID = "default_profile_id"
        case recoveryMessage = "recovery_message"
    }

    init(_ summary: TerminalProfileSettingsSummary) {
        profiles = summary.profiles
        defaultProfileID = summary.defaultProfileID
        recoveryMessage = summary.recoveryMessage
    }
}

private struct ShellCoreManagedTerminalAccountSettingsSummaryPayload: Encodable {
    let plans: [ShellCoreManagedTerminalAccountPlanPayload]

    init(_ summary: ManagedTerminalAccountSettingsSummary) {
        plans = summary.plans.map(ShellCoreManagedTerminalAccountPlanPayload.init)
    }
}

private struct ShellCoreManagedTerminalAccountPlanPayload: Encodable {
    let request: ShellCoreManagedTerminalAccountRequestPayload
    let status: ShellCoreManagedTerminalAccountPlanStatusPayload
    let steps: [ShellCoreManagedTerminalAccountPlanStepPayload]

    init(_ plan: ManagedTerminalAccountPlan) {
        request = ShellCoreManagedTerminalAccountRequestPayload(plan.request)
        status = ShellCoreManagedTerminalAccountPlanStatusPayload(plan.status)
        steps = plan.steps.map(ShellCoreManagedTerminalAccountPlanStepPayload.init)
    }
}

private struct ShellCoreManagedTerminalAccountRequestPayload: Encodable {
    let accountName: String
    let guiUserName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool
    let bindCurrentSpaceAfterSuccess: Bool

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case guiUserName = "gui_user_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
        case bindCurrentSpaceAfterSuccess = "bind_current_space_after_success"
    }

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        guiUserName = request.guiUserName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
        bindCurrentSpaceAfterSuccess = request.bindCurrentSpaceAfterSuccess
    }
}

private struct ShellCoreManagedTerminalAccountPlanStatusPayload: Encodable {
    let type: String
    let errors: [ShellCoreManagedTerminalAccountValidationErrorPayload]?
    let path: String?
    let profileID: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case errors
        case path
        case profileID = "profile_id"
    }

    init(_ status: ManagedTerminalAccountPlanStatus) {
        switch status {
        case .readyToApply:
            type = "ready_to_apply"
            errors = nil
            path = nil
            profileID = nil
        case .alreadyReady:
            type = "already_ready"
            errors = nil
            path = nil
            profileID = nil
        case .repair:
            type = "repair"
            errors = nil
            path = nil
            profileID = nil
        case let .invalid(validationErrors):
            type = "invalid"
            errors = validationErrors.map(
                ShellCoreManagedTerminalAccountValidationErrorPayload.init
            )
            path = nil
            profileID = nil
        case .helperUnavailable:
            type = "helper_unavailable"
            errors = nil
            path = nil
            profileID = nil
        case .accountNotAlanManaged:
            type = "account_not_alan_managed"
            errors = nil
            path = nil
            profileID = nil
        case let .legacySudoersPresent(legacyPath):
            type = "legacy_sudoers_present"
            errors = nil
            path = legacyPath
            profileID = nil
        case .ptySpawnFailed:
            type = "pty_spawn_failed"
            errors = nil
            path = nil
            profileID = nil
        case .requiresDestructiveConfirmation:
            type = "requires_destructive_confirmation"
            errors = nil
            path = nil
            profileID = nil
        case let .sudoersConflict(path):
            type = "sudoers_conflict"
            errors = nil
            self.path = path
            profileID = nil
        case let .terminalProfileConflict(profileID):
            type = "terminal_profile_conflict"
            errors = nil
            path = nil
            self.profileID = profileID
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(type, forKey: .type)
        try container.encodeIfPresent(errors, forKey: .errors)
        try container.encodeIfPresent(path, forKey: .path)
        try container.encodeIfPresent(profileID, forKey: .profileID)
    }
}

private struct ShellCoreManagedTerminalAccountValidationErrorPayload: Encodable {
    let type: String
    let value: String

    init(_ error: ManagedTerminalAccountValidationError) {
        switch error {
        case let .invalidAccountName(value):
            type = "invalid_account_name"
            self.value = value
        case let .invalidGUIUserName(value):
            type = "invalid_gui_user_name"
            self.value = value
        case let .reservedAccountName(value):
            type = "reserved_account_name"
            self.value = value
        case let .invalidShell(value):
            type = "invalid_shell"
            self.value = value
        case let .coreUnavailable(value):
            type = "invalid_shell"
            self.value = value
        }
    }
}

private struct ShellCoreManagedTerminalAccountPlanStepPayload: Encodable {
    let kind: String
    let summary: String
    let requiresPrivilege: Bool

    private enum CodingKeys: String, CodingKey {
        case kind
        case summary
        case requiresPrivilege = "requires_privilege"
    }

    init(_ step: ManagedTerminalAccountPlanStep) {
        kind = step.kind.shellCoreID
        summary = step.summary
        requiresPrivilege = step.requiresPrivilege
    }
}

private extension ManagedTerminalAccountPlanStepKind {
    var shellCoreID: String {
        switch self {
        case .createStandardAccount:
            return "create_standard_account"
        case .repairAccountType:
            return "repair_account_type"
        case .repairHomeDirectory:
            return "repair_home_directory"
        case .repairShell:
            return "repair_shell"
        case .hideAccount:
            return "hide_account"
        case .writeSudoersDropIn:
            return "write_sudoers_drop_in"
        case .validateSudoers:
            return "validate_sudoers"
        case .verifyTerminalEntry:
            return "verify_terminal_entry"
        case .createOrUpdateTerminalProfile:
            return "create_or_update_terminal_profile"
        case .bindCurrentSpace:
            return "bind_current_space"
        case .removeSudoersDropIn:
            return "remove_sudoers_drop_in"
        case .removeManagedTerminalProfile:
            return "remove_managed_terminal_profile"
        case .deleteAccount:
            return "delete_account"
        case .deleteHomeDirectory:
            return "delete_home_directory"
        case let .helperStep(kind):
            return kind.rawValue
        }
    }
}

private struct ShellCoreCapabilitiesSettingsSummaryPayload: Encodable {
    let skills: [ShellCoreSettingsSkillSummaryPayload]
    let unavailableReason: String?

    private enum CodingKeys: String, CodingKey {
        case skills
        case unavailableReason = "unavailable_reason"
    }

    init(_ summary: ShellSettingsCapabilitiesSummary) {
        skills = summary.skills.map(ShellCoreSettingsSkillSummaryPayload.init)
        unavailableReason = summary.unavailableReason
    }
}

private struct ShellCoreSettingsSkillSummaryPayload: Encodable {
    let id: String
    let name: String
    let enabled: Bool
    let allowImplicitInvocation: Bool
    let available: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case enabled
        case allowImplicitInvocation = "allow_implicit_invocation"
        case available
    }

    init(_ summary: ShellSettingsSkillSummary) {
        id = summary.id
        name = summary.name
        enabled = summary.enabled
        allowImplicitInvocation = summary.allowImplicitInvocation
        available = summary.available
    }
}

private struct ShellCoreLocalRowsPayload: Encodable {
    let local: ShellCoreLocalSettingsSummaryPayload
    let diagnostics: ShellCoreDiagnosticsSettingsSummaryPayload

    init(local: ShellSettingsLocalSummary, diagnostics: ShellSettingsDiagnosticsSummary) {
        self.local = ShellCoreLocalSettingsSummaryPayload(local)
        self.diagnostics = ShellCoreDiagnosticsSettingsSummaryPayload(diagnostics)
    }
}

private struct ShellCoreLocalSettingsSummaryPayload: Encodable {
    let bundleIdentifier: String
    let channelLabel: String
    let cliToolName: String
    let daemonURL: String
    let daemonBindAddress: String
    let updateSummary: String
    let updateDetail: String
    let alanHomeDisplayPath: String
    let applicationSupportDisplayPath: String
    let globalSkillsDisplayPath: String
    let shellControlNamespace: String

    private enum CodingKeys: String, CodingKey {
        case bundleIdentifier = "bundle_identifier"
        case channelLabel = "channel_label"
        case cliToolName = "cli_tool_name"
        case daemonURL = "daemon_url"
        case daemonBindAddress = "daemon_bind_address"
        case updateSummary = "update_summary"
        case updateDetail = "update_detail"
        case alanHomeDisplayPath = "alan_home_display_path"
        case applicationSupportDisplayPath = "application_support_display_path"
        case globalSkillsDisplayPath = "global_skills_display_path"
        case shellControlNamespace = "shell_control_namespace"
    }

    init(_ summary: ShellSettingsLocalSummary) {
        bundleIdentifier = summary.bundleIdentifier
        channelLabel = summary.channelLabel
        cliToolName = summary.cliToolName
        daemonURL = summary.daemonURL
        daemonBindAddress = summary.daemonBindAddress
        updateSummary = summary.updateSummary
        updateDetail = summary.updateDetail
        alanHomeDisplayPath = summary.alanHomeDisplayPath
        applicationSupportDisplayPath = summary.applicationSupportDisplayPath
        globalSkillsDisplayPath = summary.globalSkillsDisplayPath
        shellControlNamespace = summary.shellControlNamespace
    }
}

private struct ShellCoreDiagnosticsSettingsSummaryPayload: Encodable {
    let isEnabled: Bool
    let retainedEventCount: UInt32
    let stutterMarkerCount: UInt32
    let lastExportURL: String?

    private enum CodingKeys: String, CodingKey {
        case isEnabled = "is_enabled"
        case retainedEventCount = "retained_event_count"
        case stutterMarkerCount = "stutter_marker_count"
        case lastExportURL = "last_export_url"
    }

    init(_ summary: ShellSettingsDiagnosticsSummary) {
        isEnabled = summary.isEnabled
        retainedEventCount = Self.clampedUInt32(summary.retainedEventCount)
        stutterMarkerCount = Self.clampedUInt32(summary.stutterMarkerCount)
        lastExportURL = summary.lastExportURL?.path
    }

    private static func clampedUInt32(_ value: Int) -> UInt32 {
        UInt32(min(max(value, 0), Int(UInt32.max)))
    }
}
