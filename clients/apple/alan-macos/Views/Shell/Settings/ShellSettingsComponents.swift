import SwiftUI

struct ShellSettingsSectionView: View {
    let section: ShellSettingsGroupSectionModel
    @Binding var appearanceMode: ShellAppearanceMode
    let sidebarVisible: Binding<Bool>
    @Binding var dimsInactiveSplitPanes: Bool
    let performanceDiagnosticsEnabled: Binding<Bool>
    let onExportPerformanceDiagnostics: () -> Void
    let onRowAction: (ShellSettingsRowModel, ShellSettingsRowActionKind) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 14) {
                Text(section.title.uppercased())
                    .font(ShellSettingsTypography.sectionTitle)
                    .foregroundStyle(ShellPalette.settingsSecondaryInk)
                    .tracking(0.4)
                    .lineLimit(1)

                Rectangle()
                    .fill(ShellPalette.line.opacity(0.20))
                    .frame(height: 0.8)
            }
            .padding(.bottom, ShellSettingsMetrics.sectionTitleBottomPadding)

            VStack(spacing: 0) {
                ForEach(Array(section.rows.enumerated()), id: \.element.id) { index, row in
                    if index > 0 {
                        ShellSettingsDivider()
                    }

                    rowView(row)
                }
            }
        }
    }

    @ViewBuilder
    private func rowView(_ row: ShellSettingsRowModel) -> some View {
        switch row.id {
        case "appearance":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Picker("Appearance", selection: $appearanceMode) {
                    ForEach(ShellAppearanceMode.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .controlSize(.small)
                .frame(width: ShellSettingsMetrics.segmentedControlWidth)
            }
        case "sidebar":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle(row.title, isOn: sidebarVisible)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }
        case "inactiveSplitDimming":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle(row.title, isOn: $dimsInactiveSplitPanes)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }
        case "performanceDiagnostics":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle(row.title, isOn: performanceDiagnosticsEnabled)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }
        case "performanceDiagnosticsExport":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Button("Export", action: onExportPerformanceDiagnostics)
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(!performanceDiagnosticsEnabled.wrappedValue)
                    .opacity(
                        performanceDiagnosticsEnabled.wrappedValue
                            ? 1
                            : ShellSettingsMetrics.disabledButtonOpacity
                    )
            }
        case "applicationSupport", "dataRoot":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.value
            ) {
                ShellSettingsPathAction(value: row.value)
            }
        default:
            if !row.actions.isEmpty {
                ShellSettingsRow(
                    systemName: row.systemName,
                    title: row.title,
                    detail: row.detail
                ) {
                    ShellSettingsRowActionAccessory(row: row, onAction: onRowAction)
                }
            } else if let detail = row.detail, row.value != nil {
                ShellSettingsRow(
                    systemName: row.systemName,
                    title: row.title,
                    detail: detail
                ) {
                    ShellSettingsValueLabel(
                        value: row.value,
                        mutability: row.mutability
                    )
                }
            } else {
                ShellSettingsRow(
                    systemName: row.systemName,
                    title: row.title,
                    detail: row.detail ?? row.value
                )
            }
        }
    }
}

private struct ShellSettingsRowHoveredKey: EnvironmentKey {
    static let defaultValue = false
}

private extension EnvironmentValues {
    var shellSettingsRowHovered: Bool {
        get { self[ShellSettingsRowHoveredKey.self] }
        set { self[ShellSettingsRowHoveredKey.self] = newValue }
    }
}

private struct ShellSettingsRow<Accessory: View>: View {
    @State private var isHovered = false

    let systemName: String
    let title: String
    let detail: String?
    @ViewBuilder let accessory: () -> Accessory

    init(
        systemName: String,
        title: String,
        detail: String?,
        @ViewBuilder accessory: @escaping () -> Accessory
    ) {
        self.systemName = systemName
        self.title = title
        self.detail = detail
        self.accessory = accessory
    }

    var body: some View {
        HStack(alignment: .center, spacing: ShellSettingsMetrics.rowColumnSpacing) {
            VStack(alignment: .leading, spacing: ShellSettingsMetrics.rowTextSpacing) {
                Text(title)
                    .font(ShellSettingsTypography.rowTitle)
                    .foregroundStyle(ShellPalette.settingsPrimaryInk)
                    .lineLimit(1)

                if let detail,
                   !detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                {
                    Text(detail)
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                        .lineSpacing(1)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .layoutPriority(1)

            accessoryView
                .environment(\.shellSettingsRowHovered, isHovered)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, ShellSettingsMetrics.rowVerticalPadding)
        .frame(minHeight: rowMinHeight)
        .onHover { isHovered = $0 }
    }

    private var rowMinHeight: CGFloat {
        let hasDetail = detail?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        return hasDetail ? ShellSettingsMetrics.rowMinHeightWithDetail : ShellSettingsMetrics.rowMinHeight
    }

    @ViewBuilder
    private var accessoryView: some View {
        accessory()
            .font(ShellSettingsTypography.accessory)
            .frame(width: ShellSettingsMetrics.accessoryColumnWidth, alignment: .trailing)
    }
}

private extension ShellSettingsRow where Accessory == EmptyView {
    init(
        systemName: String,
        title: String,
        detail: String?
    ) {
        self.systemName = systemName
        self.title = title
        self.detail = detail
        self.accessory = { EmptyView() }
    }
}

private struct ShellSettingsValueLabel: View {
    let value: String?
    let mutability: ShellSettingsRowMutability

    var body: some View {
        HStack(spacing: 6) {
            Text(value ?? "Unavailable")
                .font(ShellSettingsTypography.value)
                .foregroundStyle(valueStyle)
                .lineLimit(1)
                .truncationMode(.middle)
                .multilineTextAlignment(.trailing)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: ShellSettingsMetrics.valueColumnWidth, alignment: .trailing)
        .help(value ?? "Unavailable")
    }

    private var valueStyle: some ShapeStyle {
        if value == "Unavailable" {
            return AnyShapeStyle(ShellPalette.settingsDisabledInk)
        }
        if mutability == .actionOnly {
            return AnyShapeStyle(ShellPalette.settingsValueInk)
        }
        return AnyShapeStyle(ShellPalette.settingsValueInk)
    }
}

private struct ShellSettingsInlineValueAction: View {
    @Environment(\.shellSettingsRowHovered) private var isRowHovered

    let isEnabled: Bool
    let buttonSystemName: String
    let buttonHelp: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Copy", systemImage: buttonSystemName)
                .labelStyle(.titleAndIcon)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .disabled(!isEnabled)
        .opacity(isRowHovered || isEnabled ? 1 : ShellSettingsMetrics.disabledButtonOpacity)
        .help(buttonHelp)
    }
}

private struct ShellSettingsPathAction: View {
    let value: String?

    var body: some View {
        Button("Show…") {
            shellSettingsOpenFolder(value)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .disabled(!ShellLocalFolderOpener.canOpenFolder(displayPath: value))
        .help(value ?? "Folder unavailable")
    }
}

private struct ShellSettingsRowActionAccessory: View {
    let row: ShellSettingsRowModel
    let onAction: (ShellSettingsRowModel, ShellSettingsRowActionKind) -> Void

    var body: some View {
        HStack(spacing: ShellSettingsMetrics.inlineActionSpacing) {
            if let value = row.value,
               !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(value)
                    .font(ShellSettingsTypography.value)
                    .foregroundStyle(ShellPalette.settingsValueInk)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 96, alignment: .trailing)
            }

            if row.actions.count == 1,
               let action = row.actions.first {
                Button {
                    onAction(row, action.id)
                } label: {
                    Label(action.title, systemImage: action.systemName)
                        .labelStyle(.iconOnly)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help(action.title)
            } else {
                Menu {
                    ForEach(row.actions) { action in
                        Button {
                            onAction(row, action.id)
                        } label: {
                            Label(action.title, systemImage: action.systemName)
                        }
                    }
                } label: {
                    Label("Actions", systemImage: "ellipsis.circle")
                        .labelStyle(.iconOnly)
                }
                .menuStyle(.button)
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help("Actions")
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

struct ShellManagedUserActionSheetState: Identifiable, Equatable {
    let action: ShellSettingsRowActionKind
    let plan: ManagedTerminalAccountPlan
    let allowsApply: Bool

    var id: String {
        "\(action.rawValue)-\(plan.request.accountName)-\(plan.steps.map(\.kind).count)"
    }

    var title: String {
        switch action {
        case .create:
            return "Create Managed User"
        case .review:
            return "Review Managed User"
        case .repair:
            return "Repair Managed User"
        case .verify:
            return "Verify Managed User"
        case .remove:
            return "Remove Managed User"
        case .installHelper, .updateHelper, .uninstallHelper:
            return "Managed User Helper"
        }
    }

    var applyTitle: String {
        switch action {
        case .remove:
            return "Remove"
        case .repair:
            return "Apply Repair"
        case .create:
            return "Create"
        case .review, .verify:
            return "Apply"
        case .installHelper, .updateHelper, .uninstallHelper:
            return "Apply"
        }
    }
}

struct ShellManagedUserCreationSheet: View {
    @Binding var draft: ManagedTerminalUserCreationDraft
    let previewResult: ManagedTerminalUserCreationPreviewResult?
    let diagnostics: [String]
    let isApplying: Bool
    let onDraftChanged: () -> Void
    let onPreview: () -> Void
    let onApply: (ManagedTerminalUserCreationPreview) -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Create Managed User")
                .font(ShellSettingsTypography.pageTitle)
                .foregroundStyle(ShellPalette.settingsPrimaryInk)

            VStack(alignment: .leading, spacing: 10) {
                TextField("Unix user", text: $draft.unixUserName)
                TextField("Display label", text: $draft.displayLabel)
            }
            .textFieldStyle(.roundedBorder)
            .disabled(isApplying)

            previewContent

            HStack {
                Button("Cancel", role: .cancel, action: onCancel)
                    .disabled(isApplying)
                Spacer()
                if isApplying {
                    ProgressView()
                        .controlSize(.small)
                }
                Button("Review Plan", action: onPreview)
                    .disabled(isApplying)
                Button("Apply") {
                    if let preview = previewResult?.preview {
                        onApply(preview)
                    }
                }
                .disabled(isApplying || (previewResult?.preview?.plan.steps.isEmpty ?? true))
            }
        }
        .padding(24)
        .frame(width: 460, alignment: .leading)
        .onChange(of: draft) { _, _ in
            onDraftChanged()
        }
    }

    @ViewBuilder
    private var previewContent: some View {
        if let result = previewResult,
           !result.errors.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(result.errors.map(errorMessage), id: \.self) { message in
                    Label(message, systemImage: "exclamationmark.triangle")
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }
        } else if let preview = previewResult?.preview {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(preview.visiblePlanRows, id: \.self) { row in
                    Label(row, systemImage: "checkmark")
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }
        }

        if !diagnostics.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(diagnostics, id: \.self) { diagnostic in
                    Text(diagnostic)
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }
        }
    }

    private func errorMessage(_ error: ManagedTerminalUserCreationPreviewError) -> String {
        switch error {
        case .missingUnixUserName:
            return "Unix user is required."
        case .missingDisplayLabel:
            return "Display label is required."
        case .duplicateUnixUser(let user):
            return "\(user) already exists."
        case .terminalProfileConflict(let profileID):
            return "Terminal Profile \(profileID) already exists."
        case .validation:
            return "Use a valid local Unix user name."
        }
    }
}

struct ShellManagedUserPlanSheet: View {
    let sheet: ShellManagedUserActionSheetState
    let diagnostics: [String]
    let isApplying: Bool
    let onApply: () -> Void
    let onCancel: () -> Void

    private var preview: ManagedTerminalUserCreationPreview {
        ManagedTerminalUserCreationPreview(request: sheet.plan.request, plan: sheet.plan)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(sheet.title)
                .font(ShellSettingsTypography.pageTitle)
                .foregroundStyle(ShellPalette.settingsPrimaryInk)

            Text("\(sheet.plan.request.fullName ?? sheet.plan.request.accountName) · \(planStatusText)")
                .font(ShellSettingsTypography.rowDetail)
                .foregroundStyle(ShellPalette.settingsSecondaryInk)

            VStack(alignment: .leading, spacing: 6) {
                ForEach(preview.visiblePlanRows, id: \.self) { row in
                    Label(row, systemImage: "checkmark")
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }

            if !diagnostics.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(diagnostics, id: \.self) { diagnostic in
                        Text(diagnostic)
                            .font(ShellSettingsTypography.rowDetail)
                            .foregroundStyle(ShellPalette.settingsSecondaryInk)
                    }
                }
            }

            HStack {
                Button("Close", role: .cancel, action: onCancel)
                    .disabled(isApplying)
                Spacer()
                if isApplying {
                    ProgressView()
                        .controlSize(.small)
                }
                if sheet.allowsApply {
                    Button(sheet.applyTitle, action: onApply)
                        .disabled(isApplying || sheet.plan.steps.isEmpty)
                }
            }
        }
        .padding(24)
        .frame(width: 460, alignment: .leading)
    }

    private var planStatusText: String {
        switch sheet.plan.status {
        case .alreadyReady:
            return "Ready"
        case .readyToApply:
            return "Ready to apply"
        case .repair:
            return "Repairable"
        case .helperUnavailable:
            return "Helper unavailable"
        case .accountNotAlanManaged:
            return "Not managed"
        case .ptySpawnFailed:
            return "PTY failed"
        case .invalid:
            return "Invalid"
        case .requiresDestructiveConfirmation:
            return "Needs confirmation"
        case .terminalProfileConflict:
            return "Terminal Profile conflict"
        }
    }
}

@MainActor
private func shellSettingsCopyToPasteboard(_ value: String?) {
    guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
          !value.isEmpty
    else {
        return
    }
    ShellSystemPasteboard().writeString(value)
}

@MainActor
private func shellSettingsOpenFolder(_ value: String?) {
    ShellLocalFolderOpener.openFolder(displayPath: value)
}

private struct ShellSettingsDivider: View {
    var body: some View {
        Divider()
            .opacity(0.45)
            .padding(.leading, ShellSettingsMetrics.rowDividerLeadingPadding)
    }
}

enum ShellSettingsMetrics {
    static let navigationWidth: CGFloat = 188
    static let navigationLeadingPadding: CGFloat = 12
    static let navigationTrailingPadding: CGFloat = 8
    static let navigationTopPadding: CGFloat = 24
    static let navigationRowHeight: CGFloat = 30
    static let navigationRowSpacing: CGFloat = 2
    static let navigationRowHorizontalPadding: CGFloat = 8
    static let navigationRowContentSpacing: CGFloat = 12
    static let navigationIconSlotWidth: CGFloat = 18
    static let navigationSelectionCornerRadius: CGFloat = 7
    static let contentWidth: CGFloat = 760
    static let detailContentLeadingPadding: CGFloat = 48
    static let detailContentTrailingPadding: CGFloat = 48
    static let detailContentTopPadding: CGFloat = 42
    static let detailContentBottomPadding: CGFloat = 40
    static let pageTitleToSectionsSpacing: CGFloat = 26
    static let sectionSpacing: CGFloat = 28
    static let sectionTitleBottomPadding: CGFloat = 10
    static let rowVerticalPadding: CGFloat = 8
    static let rowMinHeight: CGFloat = 48
    static let rowMinHeightWithDetail: CGFloat = 56
    static let agentSummaryRowMinHeight: CGFloat = 58
    static let rowTextSpacing: CGFloat = 1
    static let rowColumnSpacing: CGFloat = 20
    static let rowDividerLeadingPadding: CGFloat = 0
    static let accessoryColumnWidth: CGFloat = 188
    static let valueColumnWidth: CGFloat = 220
    static let inlineActionSpacing: CGFloat = 8
    static let inlineIconButtonSize: CGFloat = 22
    static let segmentedControlWidth: CGFloat = 196
    static let disabledButtonOpacity: CGFloat = 0.55
}

enum ShellSettingsTypography {
    static let navigationIcon = Font.system(size: 13, weight: .regular)

    static func navigationLabel(selected: Bool) -> Font {
        .system(size: 13, weight: selected ? .semibold : .regular)
    }

    static let pageTitle = Font.system(size: 22, weight: .semibold)
    static let sectionTitle = Font.system(size: 11, weight: .semibold)
    static let rowTitle = Font.system(size: 13, weight: .semibold)
    static let rowDetail = Font.system(size: 12, weight: .regular)
    static let accessory = Font.system(size: 13, weight: .regular)
    static let value = Font.system(size: 13, weight: .medium)
    static let agentName = Font.system(size: 15, weight: .semibold)
    static let badge = Font.system(size: 11.5, weight: .medium)
    static let valueActionIcon = Font.system(size: 9.5, weight: .semibold)
    static let inlineActionIcon = Font.system(size: 10.5, weight: .medium)
    static let actionButton = Font.system(size: 12.3, weight: .medium)
}
