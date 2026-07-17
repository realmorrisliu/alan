import SwiftUI

struct ShellSettingsBackdrop: View {
    var body: some View {
        ZStack {
            ShellPalette.settingsPane
            ShellPalette.windowBackdropTint.opacity(0.025)
        }
    }
}

struct ShellSettingsNavigationRailBackground: View {
    var body: some View {
        Color.clear
    }
}

struct ShellSettingsDetailBackground: View {
    var body: some View {
        Color.clear
    }
}

struct ShellSettingsNavigationView: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hoveredGroup: ShellSettingsNavigationGroup?

    let groups: [ShellSettingsNavigationGroupModel]
    @Binding var selectedGroup: ShellSettingsNavigationGroup

    var body: some View {
        VStack(alignment: .leading, spacing: ShellSettingsMetrics.navigationRowSpacing) {
            ForEach(groups) { group in
                Button {
                    selectedGroup = group.id
                } label: {
                    HStack(spacing: ShellSettingsMetrics.navigationRowContentSpacing) {
                        Image(systemName: group.systemName)
                            .font(ShellSettingsTypography.navigationIcon)
                            .foregroundStyle(iconStyle(for: group))
                            .frame(width: ShellSettingsMetrics.navigationIconSlotWidth, height: 16)

                        Text(group.title)
                            .font(ShellSettingsTypography.navigationLabel(selected: group.id == selectedGroup))
                            .foregroundStyle(textStyle(for: group))
                            .lineLimit(1)

                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, ShellSettingsMetrics.navigationRowHorizontalPadding)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(height: ShellSettingsMetrics.navigationRowHeight)
                    .contentShape(Rectangle())
                    .background {
                        ShellSettingsNavigationRowBackground(
                            state: rowVisualState(for: group)
                        )
                    }
                }
                .buttonStyle(.plain)
                .help(group.title)
                .accessibilityLabel(Text(group.title))
                .onHover { isHovered in
                    if isHovered {
                        hoveredGroup = group.id
                    } else if hoveredGroup == group.id {
                        hoveredGroup = nil
                    }
                }
            }
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: hoveredGroup)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: selectedGroup)
    }

    private func iconStyle(for group: ShellSettingsNavigationGroupModel) -> some ShapeStyle {
        group.id == selectedGroup
            ? AnyShapeStyle(ShellPalette.settingsPrimaryInk)
            : AnyShapeStyle(ShellPalette.settingsSecondaryInk)
    }

    private func textStyle(for group: ShellSettingsNavigationGroupModel) -> some ShapeStyle {
        group.id == selectedGroup
            ? AnyShapeStyle(ShellPalette.settingsPrimaryInk)
            : AnyShapeStyle(ShellPalette.settingsSecondaryInk)
    }

    private func rowVisualState(
        for group: ShellSettingsNavigationGroupModel
    ) -> ShellSettingsNavigationRowVisualState {
        if group.id == selectedGroup {
            return .selected
        }

        if group.id == hoveredGroup {
            return .hover
        }

        return .normal
    }
}

private enum ShellSettingsNavigationRowVisualState: Equatable {
    case normal
    case hover
    case selected

    var fill: Color? {
        switch self {
        case .normal:
            return nil
        case .hover:
            return ShellPalette.settingsNavigationHover
        case .selected:
            return ShellPalette.settingsNavigationSelection
        }
    }

    var stroke: Color {
        switch self {
        case .normal:
            return .clear
        case .hover:
            return ShellPalette.line.opacity(0.07)
        case .selected:
            return ShellPalette.line.opacity(0.08)
        }
    }
}

private struct ShellSettingsNavigationRowBackground: View {
    let state: ShellSettingsNavigationRowVisualState

    var body: some View {
        let shape = RoundedRectangle(
            cornerRadius: ShellSettingsMetrics.navigationSelectionCornerRadius,
            style: .continuous
        )

        ZStack(alignment: .leading) {
            if let fill = state.fill {
                shape
                    .fill(fill)
                    .overlay {
                        shape.stroke(state.stroke, lineWidth: 0.5)
                    }
            }
        }
    }
}

struct ShellSettingsGroupView: View {
    let group: ShellSettingsNavigationGroupModel
    @Binding var appearanceMode: ShellAppearanceMode
    let sidebarVisible: Binding<Bool>
    @Binding var dimsInactiveSplitPanes: Bool
    let performanceDiagnosticsEnabled: Binding<Bool>
    let onExportPerformanceDiagnostics: () -> Void
    let onRowAction: (ShellSettingsRowModel, ShellSettingsRowActionKind) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ShellSettingsMetrics.pageTitleToSectionsSpacing) {
            Text(group.title)
                .font(ShellSettingsTypography.pageTitle)
                .foregroundStyle(ShellPalette.settingsPrimaryInk)

            VStack(alignment: .leading, spacing: ShellSettingsMetrics.sectionSpacing) {
                ForEach(group.sections) { section in
                    ShellSettingsSectionView(
                        section: section,
                        appearanceMode: $appearanceMode,
                        sidebarVisible: sidebarVisible,
                        dimsInactiveSplitPanes: $dimsInactiveSplitPanes,
                        performanceDiagnosticsEnabled: performanceDiagnosticsEnabled,
                        onExportPerformanceDiagnostics: onExportPerformanceDiagnostics,
                        onRowAction: onRowAction
                    )
                }
            }
        }
    }
}

private struct ShellSettingsVerticalDivider: View {
    var body: some View {
        Rectangle()
            .fill(ShellPalette.line.opacity(0.13))
            .frame(width: 0.8)
    }
}
