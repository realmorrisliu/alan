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
